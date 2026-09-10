use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use base64::{Engine as _, engine::general_purpose};
use image::{DynamicImage, imageops::FilterType};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_store::StoreExt;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

static INITIAL_FILE: Mutex<Option<String>> = Mutex::new(None);
/// 转换窗口的待处理路径：创建/通知前先写入，前端挂载后 drain，
/// 兜住「窗口已创建但 webview 尚未挂载监听」的竞态。
/// 多选批量转换时是整个路径列表。
static PENDING_CONVERT: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// 串行化 update_convert_settings 的 get+merge+set+save：两个窗口在同一 IPC 往返内
/// 并发改不同字段时，没有它就会读-改-写互相覆盖（旧整对象 set 的问题）。
static CONVERT_SETTINGS_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Clone)]
struct ImageFile {
    name: String,
    path: String,
}

#[derive(Serialize, Clone)]
struct FolderContent {
    dir_path: String,
    files: Vec<ImageFile>,
}

/// 扩展名是否属于支持列表（ASCII 大小写不敏感，零分配）
fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
        .unwrap_or(false)
}

fn collect_images(dir: &Path) -> Vec<ImageFile> {
    let mut files: Vec<ImageFile> = fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| has_image_extension(&e.path()))
                .map(|e| {
                    let path = e.path();
                    ImageFile {
                        name: e.file_name().to_string_lossy().to_string(),
                        path: path.to_string_lossy().to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    files
}

/// 自然排序：把字符串按「非数字段 / 数字段」拆分后逐段比较，数字按数值比。
/// 例如 1.jpg, 2.jpg, 10.jpg, 11.jpg 会按 1,2,10,11 排列，而不是字典序 1,10,11,2。
/// 全程按字节比较（UTF-8 字节序与码点序一致），不做任何分配。
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut ai, mut bi) = (0usize, 0usize);

    while ai < a.len() && bi < b.len() {
        let (ac, bc) = (a[ai], b[bi]);

        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let a_start = ai;
            while ai < a.len() && a[ai].is_ascii_digit() {
                ai += 1;
            }
            let b_start = bi;
            while bi < b.len() && b[bi].is_ascii_digit() {
                bi += 1;
            }

            // 数值比较：先比去掉前导零后的长度，再比字典序（等价于数值比且不分配/不溢出）
            let a_digits = trim_leading_zeros(&a[a_start..ai]);
            let b_digits = trim_leading_zeros(&b[b_start..bi]);
            let ord = a_digits
                .len()
                .cmp(&b_digits.len())
                .then_with(|| a_digits.cmp(b_digits));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            let ord = ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
            ai += 1;
            bi += 1;
        }
    }

    a.len().cmp(&b.len())
}

fn trim_leading_zeros(digits: &[u8]) -> &[u8] {
    let mut i = 0;
    while i + 1 < digits.len() && digits[i] == b'0' {
        i += 1;
    }
    &digits[i..]
}

#[cfg(test)]
mod tests {
    use super::{
        avif_container_orientation, avif_exif_orientation, convert_request, decode_image,
        image_dims, is_convert_invocation, is_owned_convert_list, merge_convert_patch, natural_cmp,
        process_image, read_convert_list, read_orientation, ConvertOptions,
    };
    use image::{DynamicImage, ImageFormat};
    use std::cmp::Ordering;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    #[test]
    fn natural_cmp_orders_numbers_numerically() {
        let mut names = vec!["10.jpg", "2.jpg", "1.jpg", "11.jpg"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1.jpg", "2.jpg", "10.jpg", "11.jpg"]);
    }

    #[test]
    fn natural_cmp_is_case_insensitive_and_prefix_aware() {
        assert_eq!(natural_cmp("IMG2.png", "img10.png"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a1b"), Ordering::Less);
        assert_eq!(natural_cmp("第2页.png", "第10页.png"), Ordering::Less);
    }

    // ---- CLI 转换参数解析（--convert / --convert-list） ----

    static UNIQUE: AtomicU32 = AtomicU32::new(0);

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    /// 临时目录下的唯一文件，Drop 时清理（断言失败也不留垃圾）。
    struct TempFile(PathBuf);

    impl TempFile {
        /// 保留调用方给的前缀/后缀，仅插入进程内唯一片段，便于测试清理守卫。
        fn new(name: &str) -> Self {
            let seq = UNIQUE.fetch_add(1, AtomicOrdering::Relaxed);
            let unique = format!("{}-{}", std::process::id(), seq);
            let file_name = match name.strip_suffix(".txt") {
                Some(stem) => format!("{}-{}.txt", stem, unique),
                None => format!("{}-{}", name, unique),
            };
            Self(std::env::temp_dir().join(file_name))
        }

        /// 与 `new` 相同，但把唯一片段插在扩展名之前（扩展名有语义时用，如 `.jpg`）。
        fn with_extension(stem: &str, ext: &str) -> Self {
            let seq = UNIQUE.fetch_add(1, AtomicOrdering::Relaxed);
            let unique = format!("{}-{}", std::process::id(), seq);
            Self(std::env::temp_dir().join(format!("{}-{}.{}", stem, unique, ext)))
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, text: &str) {
            std::fs::write(&self.0, text).unwrap();
        }

        fn write_bytes(&self, bytes: &[u8]) {
            std::fs::write(&self.0, bytes).unwrap();
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn convert_request_keeps_only_images() {
        assert_eq!(
            convert_request(&argv(&["app.exe", "--convert", "a.JPG"])),
            Some(vec!["a.JPG".to_string()])
        );
        assert_eq!(
            convert_request(&argv(&["app.exe", "--convert", "a.png", "b.txt", "c.webp"])),
            Some(vec!["a.png".to_string(), "c.webp".to_string()])
        );
        assert_eq!(convert_request(&argv(&["app.exe", "--convert", "a.txt"])), None);
        assert_eq!(convert_request(&argv(&["app.exe", "--convert"])), None);
        assert_eq!(convert_request(&argv(&["app.exe", "--convert-list"])), None);
        assert_eq!(convert_request(&argv(&["app.exe", "a.png"])), None);
        assert_eq!(convert_request(&argv(&[])), None);
    }

    #[test]
    fn convert_request_parses_list_file_and_filters() {
        let file = TempFile::new("hive-convert-list.txt");
        file.write("C:\\imgs\\a.jpg\r\n\r\nC:\\imgs\\b.txt\nC:\\imgs\\c.avif\n");
        let arg = file.path().to_string_lossy().to_string();
        assert_eq!(
            convert_request(&argv(&["app.exe", "--convert-list", &arg])),
            Some(vec!["C:\\imgs\\a.jpg".to_string(), "C:\\imgs\\c.avif".to_string()])
        );
        // 先读后删：自有列表文件用完即清
        assert!(!file.path().exists());
    }

    #[test]
    fn is_convert_invocation_matches_only_convert_flags() {
        assert!(is_convert_invocation(&argv(&["app.exe", "--convert", "a.jpg"])));
        assert!(is_convert_invocation(&argv(&["app.exe", "--convert"])));
        assert!(is_convert_invocation(&argv(&["app.exe", "--convert-list", "list.txt"])));
        assert!(is_convert_invocation(&argv(&["app.exe", "--convert-list"])));
        assert!(!is_convert_invocation(&argv(&["app.exe", "a.png"])));
        assert!(!is_convert_invocation(&argv(&["app.exe"])));
        assert!(!is_convert_invocation(&argv(&[])));
        // 位置一致：只认第一个参数，图片路径里的同名串不算
        assert!(!is_convert_invocation(&argv(&["app.exe", "a.png", "--convert"])));
    }

    #[test]
    fn convert_request_missing_list_file_is_none() {
        let file = TempFile::new("hive-convert-missing.txt");
        let arg = file.path().to_string_lossy().to_string();
        assert_eq!(convert_request(&argv(&["app.exe", "--convert-list", &arg])), None);
    }

    #[test]
    fn convert_list_cleanup_is_guarded() {
        // 自有临时列表文件：读后删除
        let owned = TempFile::new("hive-convert-owned.txt");
        owned.write("C:\\imgs\\a.jpg\n");
        assert_eq!(read_convert_list(&owned.path().to_string_lossy()).len(), 1);
        assert!(!owned.path().exists(), "owned temp list should be deleted");

        // 同目录但前缀不符：绝不删除，但内容照常解析
        let foreign = TempFile::new("hive-other.txt");
        foreign.write("C:\\imgs\\a.jpg\n");
        assert_eq!(
            read_convert_list(&foreign.path().to_string_lossy()),
            vec!["C:\\imgs\\a.jpg".to_string()]
        );
        assert!(foreign.path().exists(), "non-hive-convert temp file must survive");

        // 前缀相符但不在临时目录根下（此处为临时目录的子目录）：绝不删除，但内容照常解析
        let seq = UNIQUE.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!("hive-test-dir-{}-{}", std::process::id(), seq));
        std::fs::create_dir_all(&dir).unwrap();
        let nested = dir.join("hive-convert-nested.txt");
        std::fs::write(&nested, "C:\\imgs\\a.jpg\n").unwrap();
        assert_eq!(
            read_convert_list(&nested.to_string_lossy()),
            vec!["C:\\imgs\\a.jpg".to_string()]
        );
        assert!(nested.exists(), "same-prefix file outside temp dir root must survive");
        let _ = std::fs::remove_dir_all(&dir);

        // 谓词本身：任意非临时目录路径都不算自有文件
        assert!(!is_owned_convert_list(Path::new("D:\\some\\dir\\hive-convert-x.txt")));
    }

    // ---- 解码按内容嗅探（扩展名不可信） ----

    #[test]
    fn decode_image_sniffs_content_not_extension() {
        // PNG 数据 + .jpg 扩展名：扩展名撒谎时也要能解码
        let file = TempFile::with_extension("hive-misnamed", "jpg");
        let mut png = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(3, 2).write_to(&mut png, ImageFormat::Png).unwrap();
        std::fs::write(file.path(), png.get_ref()).unwrap();

        // image::open 按扩展名选解码器：PNG 数据喂给 JPEG 解码器必然失败
        assert!(image::open(file.path()).is_err(), "extension-based open must fail");

        let img = decode_image(&file.path().to_string_lossy()).unwrap();
        assert_eq!((img.width(), img.height()), (3, 2));
    }

    // ---- 转换设置的原子 patch（KNOWN_ISSUES item 2 后端） ----

    #[test]
    fn merge_convert_patch_merges_disjoint_fields() {
        let current = serde_json::json!({ "format": "png", "quality": 80 });
        let patch = serde_json::json!({ "rotation": "cw90", "prefix": "hive_" });
        let merged = merge_convert_patch(&current, &patch).unwrap();
        assert_eq!(merged["format"], serde_json::json!("png"));
        assert_eq!(merged["quality"], serde_json::json!(80));
        assert_eq!(merged["rotation"], serde_json::json!("cw90"));
        assert_eq!(merged["prefix"], serde_json::json!("hive_"));
    }

    #[test]
    fn merge_convert_patch_rejects_unknown_keys() {
        let err = merge_convert_patch(&serde_json::json!({}), &serde_json::json!({ "evil": 1 }))
            .unwrap_err();
        assert!(err.contains("evil"), "unexpected error: {}", err);
    }

    #[test]
    fn merge_convert_patch_allows_null_width_and_height() {
        let current = serde_json::json!({ "width": 800, "height": 600 });
        let merged = merge_convert_patch(&current, &serde_json::json!({ "width": null })).unwrap();
        assert!(merged["width"].is_null(), "width should be cleared: {}", merged);
        assert_eq!(merged["height"], serde_json::json!(600));
    }

    #[test]
    fn merge_convert_patch_clamps_quality() {
        let hi = merge_convert_patch(&serde_json::json!({}), &serde_json::json!({ "quality": 250 }))
            .unwrap();
        assert_eq!(hi["quality"], serde_json::json!(100));
        let lo = merge_convert_patch(&serde_json::json!({}), &serde_json::json!({ "quality": -3 }))
            .unwrap();
        assert_eq!(lo["quality"], serde_json::json!(1));
        let rounded =
            merge_convert_patch(&serde_json::json!({}), &serde_json::json!({ "quality": 82.6 }))
                .unwrap();
        assert_eq!(rounded["quality"], serde_json::json!(83));
    }

    #[test]
    fn merge_convert_patch_rejects_non_object_patch() {
        assert!(merge_convert_patch(&serde_json::json!({}), &serde_json::json!([1, 2])).is_err());
        assert!(merge_convert_patch(&serde_json::json!({}), &serde_json::json!("nope")).is_err());
    }

    // ---- AVIF 输入（image avif-native + vendor/dav1d-shim） ----

    /// 用 image 的 AVIF 编码器（ravif）生成 8-bit 测试文件。
    fn write_avif(name: &str, img: &DynamicImage) -> TempFile {
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Avif).unwrap();
        let file = TempFile::with_extension(name, "avif");
        file.write_bytes(buf.get_ref());
        file
    }

    fn avif_path(file: &TempFile) -> String {
        file.path().to_string_lossy().to_string()
    }

    fn solid_rgba(w: u32, h: u32, color: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(w, h, image::Rgba(color)))
    }

    /// 带梯度：让 mdat 里有真实码流，截断/损坏类测试才有意义。
    fn gradient_rgba(w: u32, h: u32) -> DynamicImage {
        let mut img = image::RgbaImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([
                (x * 17 % 256) as u8,
                (y * 29 % 256) as u8,
                ((x + y) * 13 % 256) as u8,
                255,
            ]);
        }
        DynamicImage::ImageRgba8(img)
    }

    fn convert_opts(rotation: &str, resize_mode: &str) -> ConvertOptions {
        ConvertOptions {
            rotation: rotation.to_string(),
            resize_mode: resize_mode.to_string(),
            width: None,
            height: None,
            pad_color: None,
            format: "png".to_string(),
            lossless: false,
            quality: None,
            prefix: String::new(),
        }
    }

    #[test]
    fn decode_image_accepts_avif_input() {
        let file = write_avif("hive-avif-decode", &solid_rgba(8, 6, [200, 60, 40, 255]));
        let img = decode_image(&avif_path(&file)).unwrap();
        assert_eq!((img.width(), img.height()), (8, 6));
        // 有损编码：颜色大致保留即可
        let p = img.to_rgba8().get_pixel(0, 0).0;
        assert!((p[0] as i32 - 200).abs() < 40, "red drift: {:?}", p);
        assert!((p[1] as i32 - 60).abs() < 40, "green drift: {:?}", p);
        assert!((p[2] as i32 - 40).abs() < 40, "blue drift: {:?}", p);
        assert!(p[3] > 200, "opaque alpha expected, got {:?}", p);
    }

    #[test]
    fn avif_alpha_round_trip() {
        let file = write_avif("hive-avif-alpha", &solid_rgba(4, 4, [180, 40, 220, 128]));
        let img = decode_image(&avif_path(&file)).unwrap();
        assert_eq!((img.width(), img.height()), (4, 4));
        let p = img.to_rgba8().get_pixel(0, 0).0;
        assert!(p[3] > 80 && p[3] < 180, "lossy alpha should stay near 128, got {}", p[3]);
    }

    #[test]
    fn decode_image_rejects_animated_avif_with_clear_error() {
        // 最小 ftyp(avis) 头：is_animated_avif 应当识别，decode_image 给出明确错误
        let file = TempFile::with_extension("hive-avis", "avif");
        file.write_bytes(b"\0\0\0\x14ftypavis\0\0\0\0avis");
        let err = decode_image(&avif_path(&file)).unwrap_err();
        assert!(err.contains("avis"), "unexpected error: {}", err);
    }

    #[test]
    fn process_image_avif_rotate_resize() {
        let file = write_avif("hive-avif-process", &gradient_rgba(16, 8));
        let path = avif_path(&file);

        // cw90：16x8 → 8x16，再 contain 进 8x8 → 4x8
        let mut o = convert_opts("cw90", "contain");
        o.width = Some(8);
        o.height = Some(8);
        let img = process_image(&path, &o).unwrap();
        assert_eq!((img.width(), img.height()), (4, 8));

        // pad：输出严格等于目标框
        o.resize_mode = "pad".to_string();
        o.width = Some(12);
        o.height = Some(12);
        o.pad_color = Some("#ff0000".to_string());
        let img = process_image(&path, &o).unwrap();
        assert_eq!((img.width(), img.height()), (12, 12));

        // crop：输出严格等于目标框
        o.rotation = "none".to_string();
        o.resize_mode = "crop".to_string();
        o.width = Some(6);
        o.height = Some(6);
        let img = process_image(&path, &o).unwrap();
        assert_eq!((img.width(), img.height()), (6, 6));
    }

    #[test]
    fn avif_malformed_inputs_return_err() {
        // 随机字节：连格式都嗅探不出
        let random = TempFile::with_extension("hive-avif-random", "avif");
        random.write_bytes(&[0xA5; 64]);
        assert!(decode_image(&avif_path(&random)).is_err());

        // 空文件
        let empty = TempFile::with_extension("hive-avif-empty", "avif");
        empty.write_bytes(&[]);
        assert!(decode_image(&avif_path(&empty)).is_err());

        // 合法 AVIF 的破坏样本：截断 / 后半清零 / 尾部涂改 —— 都必须 Err 且不 panic
        let mut buf = Cursor::new(Vec::new());
        gradient_rgba(16, 16).write_to(&mut buf, ImageFormat::Avif).unwrap();
        let bytes = buf.into_inner();
        assert!(bytes.len() > 128, "sanity: encoded avif should not be tiny");

        let truncated = TempFile::with_extension("hive-avif-truncated", "avif");
        truncated.write_bytes(&bytes[..bytes.len() / 2]);
        assert!(decode_image(&avif_path(&truncated)).is_err());

        let mut half_zeroed = bytes.clone();
        let mid = half_zeroed.len() / 2;
        half_zeroed[mid..].fill(0);
        let half_zeroed_file = TempFile::with_extension("hive-avif-halfzero", "avif");
        half_zeroed_file.write_bytes(&half_zeroed);
        assert!(decode_image(&avif_path(&half_zeroed_file)).is_err());

        // 尾部涂改：把最后一个 box 的头部（size/type）涂成 0xFF。
        // 直接涂 0xFF 到文件末尾不一定致命（AV1 解码器可能忽略尾随字节），
        // 破坏最后一个 box 的头部才能确定性地让容器解析失败。
        let mut clobbered = bytes.clone();
        let mut off = 0usize;
        while off + 8 <= clobbered.len() {
            let size = u32::from_be_bytes(clobbered[off..off + 4].try_into().unwrap()) as usize;
            if size < 8 || off + size > clobbered.len() {
                break;
            }
            if off + size == clobbered.len() {
                clobbered[off..off + 8].fill(0xFF);
                break;
            }
            off += size;
        }
        assert_ne!(clobbered, bytes, "sanity: last box header should be clobbered");
        let clobbered_file = TempFile::with_extension("hive-avif-clobber", "avif");
        clobbered_file.write_bytes(&clobbered);
        assert!(decode_image(&avif_path(&clobbered_file)).is_err());
    }

    #[test]
    fn image_dims_avif_returns_dimensions() {
        let file = write_avif("hive-avif-dims", &solid_rgba(12, 8, [10, 20, 30, 255]));
        let path = avif_path(&file);
        let dims = tauri::async_runtime::block_on(image_dims(path.clone())).unwrap();
        assert_eq!(dims, (12, 8));
        // 编码器不写 irot/imir：容器方向为 None，回退 EXIF（默认 1，不交换宽高）
        assert_eq!(avif_container_orientation(&path), None);
        assert_eq!(read_orientation(&path), 1);
    }

    #[test]
    fn avif_exif_orientation_covers_all_eight_codes() {
        use mp4parse::{ImageMirror, ImageRotation};
        // irot 逆时针：D90 = 顺时针 270° = EXIF 8
        assert_eq!(avif_exif_orientation(ImageRotation::D0, None), 1u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D90, None), 8u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D180, None), 3u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D270, None), 6u8);
        // 先旋转后镜像（MIAF §7.3.6.7）：TopBottom = 垂直镜像，LeftRight = 水平镜像
        assert_eq!(avif_exif_orientation(ImageRotation::D0, Some(ImageMirror::TopBottom)), 4u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D90, Some(ImageMirror::TopBottom)), 5u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D180, Some(ImageMirror::TopBottom)), 2u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D270, Some(ImageMirror::TopBottom)), 7u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D0, Some(ImageMirror::LeftRight)), 2u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D90, Some(ImageMirror::LeftRight)), 7u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D180, Some(ImageMirror::LeftRight)), 4u8);
        assert_eq!(avif_exif_orientation(ImageRotation::D270, Some(ImageMirror::LeftRight)), 5u8);
    }
}

#[tauri::command]
async fn open_file(app: AppHandle) -> Result<FolderContent, String> {
    // 原生对话框会阻塞线程 → 丢到 blocking 线程池，不占 async worker
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .blocking_pick_file()
            .ok_or("No file selected")?;

        let file_path = path.as_path().ok_or("Invalid path")?;
        let dir = file_path.parent().ok_or("No parent directory")?;
        let dir_path = dir.to_string_lossy().to_string();
        let files = collect_images(dir);

        Ok(FolderContent { dir_path, files })
    })
    .await
    .map_err(|e| format!("打开文件对话框失败: {}", e))?
}

#[tauri::command]
async fn open_folder(app: AppHandle) -> Result<FolderContent, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .blocking_pick_folder()
            .ok_or("No folder selected")?;

        let dir = path.as_path().ok_or("Invalid path")?;
        let dir_path = dir.to_string_lossy().to_string();
        let files = collect_images(dir);

        Ok(FolderContent { dir_path, files })
    })
    .await
    .map_err(|e| format!("打开文件夹对话框失败: {}", e))?
}

#[tauri::command(async)]
fn open_path(path: String) -> Result<FolderContent, String> {
    let file_path = PathBuf::from(&path);
    if !file_path.exists() {
        return Err("File does not exist".to_string());
    }
    let dir = file_path.parent().ok_or("No parent directory")?;
    let dir_path = dir.to_string_lossy().to_string();
    let files = collect_images(dir);
    Ok(FolderContent { dir_path, files })
}

#[tauri::command]
fn get_initial_file() -> Option<String> {
    INITIAL_FILE.lock().ok().and_then(|g| g.clone())
}

/// 取出待处理的转换路径（取出即清空），供转换窗口挂载时兜底。
#[tauri::command]
fn take_pending_convert() -> Option<Vec<String>> {
    PENDING_CONVERT.lock().ok().and_then(|mut g| g.take())
}

#[tauri::command(async)]
fn read_exif(path: String) -> Result<HashMap<String, String>, String> {
    let file = std::fs::File::open(&path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut buf_reader = std::io::BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut buf_reader) {
        Ok(exif) => exif,
        Err(exif::Error::NotFound(_)) => return Ok(HashMap::new()),
        Err(e) => return Err(format!("解析 EXIF 失败: {}", e)),
    };

    let mut map = HashMap::<String, String>::new();

    // key 统一用 EXIF tag 名（稳定标识），本地化标签由前端按界面语言映射
    for field in exif.fields() {
        let tag_name = field.tag.to_string();
        let value = field.display_value().with_unit(&exif).to_string();

        if field.tag == exif::Tag::MakerNote {
            let raw: &[u8] = match &field.value {
                exif::Value::Undefined(v, _) => v,
                exif::Value::Byte(v) => v,
                _ => &[],
            };
            let b64 = general_purpose::STANDARD.encode(raw);
            map.insert("MakerNote".to_string(), b64);
        } else {
            map.insert(tag_name, value);
        }
    }

    if map.is_empty() {
        return Ok(map);
    }

    Ok(map)
}

/// 只读文件头取图片尺寸，并按 EXIF Orientation 交换宽高。
/// 前端用它替代 `new Image()` 量尺寸，省掉「为量尺寸把整图解码一遍」。
/// 必须做方向校正：浏览器 naturalWidth/Height 是应用过方向的，语义要对齐。
#[tauri::command]
async fn image_dims(path: String) -> Result<(u32, u32), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (w, h) = image::ImageReader::open(&path)
            .map_err(|e| format!("打开图片失败: {}", e))?
            .with_guessed_format()
            .map_err(|e| format!("识别图片格式失败: {}", e))?
            .into_dimensions()
            .map_err(|e| format!("读取图片尺寸失败: {}", e))?;
        // AVIF 例外：image 的 AvifDecoder::new 会完整解一帧（没有头解析），所以对
        // AVIF 而言 into_dimensions 是全量解码。mp4parse 只把 ispe 暴露成裸指针
        // （spatial_extents_ptr）、字段私有、无安全访问器；owner 只允许 imir 处的一处
        // unsafe，因此保留 into_dimensions（前端 sizeCache 缓存，每文件只量一次）。
        let orientation = read_orientation(&path);
        Ok(if matches!(orientation, 5..=8) { (h, w) } else { (w, h) })
    })
    .await
    .map_err(|e| format!("读取图片尺寸失败: {}", e))?
}

// ---------- 图片格式转换 ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertOptions {
    rotation: String,    // exif | cw90 | ccw90 | rot180
    resize_mode: String, // none | contain | fit-width | pad | crop | stretch
    width: Option<u32>,
    height: Option<u32>,
    pad_color: Option<String>, // #RRGGBB / #RRGGBBAA，仅 pad 模式使用
    format: String, // avif | webp | jpg | png | bmp
    lossless: bool,
    quality: Option<u8>,
    prefix: String,
}

#[derive(Serialize)]
struct PreviewResult {
    path: String,
    width: u32,
    height: u32,
}

/// 文件头是否是 AVIF/AVIS 容器（ftyp box + avif/avis 品牌）。
/// 只读前 64 字节，避免把 JPEG/PNG 等丢给 mp4parse。
fn looks_like_avif(path: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 64];
    // read_exact：单次 read 可能短读；AVIF 必然长于 64 字节，读不满直接判否
    if file.read_exact(&mut head).is_err() {
        return false;
    }
    &head[4..8] == b"ftyp" && head[8..].windows(4).any(|w| w == b"avif" || w == b"avis")
}

/// avis（AVIF 动画序列）的品牌：image 的格式嗅探只认 major brand `avif`，
/// avis 会以「无法识别格式」失败；这里提前给出明确错误。
fn is_animated_avif(path: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 12];
    file.read_exact(&mut head).is_ok() && &head[4..8] == b"ftyp" && &head[8..12] == b"avis"
}

/// 容器 irot/imir → EXIF Orientation 码（1~8）。
/// MIAF §7.3.6.7 规定变换顺序为 clean aperture → rotation → mirror（先旋转后镜像）；
/// irot 的 angle 是逆时针（HEIF §6.5.10），所以 D90 显示时等于顺时针 270° = EXIF 8。
/// 组合表与 Chromium AVIF 解码器（avif_image_decoder.cc 的 kAxisAngleToOrientation）一致：
///   无镜像   angle 0/1/2/3 → 1/8/3/6
///   TopBottom（上下交换 = 垂直镜像）→ 4/5/2/7
///   LeftRight（左右交换 = 水平镜像）→ 2/7/4/5
fn avif_exif_orientation(
    rotation: mp4parse::ImageRotation,
    mirror: Option<mp4parse::ImageMirror>,
) -> u8 {
    let angle = match rotation {
        mp4parse::ImageRotation::D0 => 0,
        mp4parse::ImageRotation::D90 => 1,
        mp4parse::ImageRotation::D180 => 2,
        mp4parse::ImageRotation::D270 => 3,
    };
    match mirror {
        None => [1u8, 8, 3, 6][angle],
        Some(mp4parse::ImageMirror::TopBottom) => [4u8, 5, 2, 7][angle],
        Some(mp4parse::ImageMirror::LeftRight) => [2u8, 7, 4, 5][angle],
    }
}

/// AVIF 容器的方向变换 → EXIF Orientation 码；不是 AVIF、无变换或解析失败时返回 None，
/// 由调用方回退到 EXIF（容器有变换时跳过 EXIF，避免双重旋转）。
fn avif_container_orientation(path: &str) -> Option<u8> {
    if !looks_like_avif(path) {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let ctx = mp4parse::read_avif(&mut file, mp4parse::ParseStrictness::Normal).ok()?;
    let rotation = ctx.image_rotation().ok()?;
    let mirror_ptr = ctx.image_mirror_ptr().ok()?;
    // SAFETY: mp4parse 只提供裸指针访问器 image_mirror_ptr()，没有安全版本。
    // 为什么不用安全解析：imir 必须按 primary item 的 ipma 关联解析，mp4parse 没有
    // 暴露 item_properties/ipma 的安全访问器；裸扫 box 会在多 item 文件中错配属性，
    // 因此只能用这个裸指针（owner 允许的最后一处妥协）。
    // 指针指向 ctx 内部 item_properties 里的 ImageMirror；ctx 在本函数内一直存活，
    // 拿到指针后没有再移动/可变借用 ctx，读取期间指针有效；null 已在下面排除。
    // ImageMirror 是无字段枚举（无 Drop），ptr::read 的位拷贝副本有效，源对象仍由 ctx 持有。
    let mirror = if mirror_ptr.is_null() {
        None
    } else {
        Some(unsafe { std::ptr::read(mirror_ptr) })
    };
    if matches!(rotation, mp4parse::ImageRotation::D0) && mirror.is_none() {
        return None; // 容器没有方向变换：交给 EXIF
    }
    Some(avif_exif_orientation(rotation, mirror))
}

/// 读取 EXIF Orientation（1=正常）。
/// AVIF 优先用容器方向（irot/imir）：image 的 AVIF 解码器不应用容器变换，
/// 而部分 AVIF 的 EXIF Orientation 为空，方向只能从容器读。
/// 容器没有变换时回退到 kamadak-exif（与其它格式一致）。
fn read_orientation(path: &str) -> u32 {
    if let Some(orientation) = avif_container_orientation(path) {
        return orientation as u32;
    }
    fs::File::open(path)
        .ok()
        .and_then(|f| {
            exif::Reader::new()
                .read_from_container(&mut std::io::BufReader::new(f))
                .ok()
        })
        .and_then(|e| {
            e.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        })
        .unwrap_or(1)
}

/// 顺时针 90° 的倍数旋转（借用入参；恒等时克隆，供预览代理复用缓存）
fn rotate_by(img: &DynamicImage, turns_cw: u8) -> DynamicImage {
    match turns_cw % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img.clone(),
    }
}

/// 按 EXIF Orientation(1~8) 校正：镜像 + 旋转。5=transpose，7=transverse。
/// 返回 None 表示无需校正（orientation 1/未知），调用方可直接沿用原图避免拷贝。
fn orientation_transform(img: &DynamicImage, orientation: u32) -> Option<DynamicImage> {
    match orientation {
        2 => Some(img.fliph()),
        3 => Some(img.rotate180()),
        4 => Some(img.flipv()),
        5 => Some(img.rotate90().fliph()),
        6 => Some(img.rotate90()),
        7 => Some(img.rotate90().flipv()),
        8 => Some(img.rotate270()),
        _ => None,
    }
}

/// 旋转设置 → 顺时针 90° 的倍数（exif 的镜像分量由 apply_orientation 处理，5/7 含 90° 旋转）
fn rotation_turns(setting: &str, orientation: u32) -> u8 {
    match setting {
        "cw90" => 1,
        "rot180" => 2,
        "ccw90" => 3,
        "exif" => match orientation {
            3 => 2,
            5 | 6 => 1,
            7 | 8 => 3,
            _ => 0,
        },
        _ => 0,
    }
}

fn exif_rotate(img: DynamicImage, path: &str) -> DynamicImage {
    match orientation_transform(&img, read_orientation(path)) {
        Some(rotated) => rotated,
        None => img,
    }
}

fn need_dims(o: &ConvertOptions) -> Result<(u32, u32), String> {
    match (o.width, o.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err("需要有效的宽度和高度".to_string()),
    }
}

/// 解析 #RRGGBB / #RRGGBBAA 填充色，非法值回退白色
fn parse_pad_color(hex: Option<&str>) -> image::Rgba<u8> {
    let white = image::Rgba([255, 255, 255, 255]);
    let Some(h) = hex.map(|s| s.trim_start_matches('#')) else {
        return white;
    };
    if !h.is_ascii() {
        return white;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 => match (byte(0), byte(2), byte(4)) {
            (Some(r), Some(g), Some(b)) => image::Rgba([r, g, b, 255]),
            _ => white,
        },
        8 => match (byte(0), byte(2), byte(4), byte(6)) {
            (Some(r), Some(g), Some(b), Some(a)) => image::Rgba([r, g, b, a]),
            _ => white,
        },
        _ => white,
    }
}

/// 保持比例缩进框内后居中铺填充色底
fn pad_to(img: DynamicImage, w: u32, h: u32, f: FilterType, color: image::Rgba<u8>) -> DynamicImage {
    let resized = img.resize(w, h, f);
    let mut canvas = image::RgbaImage::from_pixel(w, h, color);
    let x = (w - resized.width()) as i64 / 2;
    let y = (h - resized.height()) as i64 / 2;
    image::imageops::overlay(&mut canvas, &resized.to_rgba8(), x, y);
    DynamicImage::ImageRgba8(canvas)
}

fn apply_resize(img: DynamicImage, o: &ConvertOptions) -> Result<DynamicImage, String> {
    let f = FilterType::Lanczos3;
    match o.resize_mode.as_str() {
        "none" => Ok(img),
        "contain" => need_dims(o).map(|(w, h)| img.resize(w, h, f)),
        "fit-width" => match o.width {
            Some(w) if w > 0 => Ok(img.resize(w, u32::MAX, f)),
            _ => Err("需要有效的宽度".to_string()),
        },
        "pad" => {
            need_dims(o).map(|(w, h)| pad_to(img, w, h, f, parse_pad_color(o.pad_color.as_deref())))
        }
        "crop" => need_dims(o).map(|(w, h)| img.resize_to_fill(w, h, f)),
        "stretch" => need_dims(o).map(|(w, h)| img.resize_exact(w, h, f)),
        other => Err(format!("未知裁剪模式: {}", other)),
    }
}

/// 按文件内容嗅探格式解码，不信任扩展名：image::open 按扩展名选解码器，
/// PNG 改名成 .jpg 会被丢给 JPEG 解码器直接失败（Chromium/image_dims 都按内容识别）。
/// AVIF 也走这条路径：image 的 avif-native 特性 + vendor/dav1d-shim（纯 Rust re_rav1d）
/// 提供解码器，Windows 上不需要 meson/ninja/NASM。grid/动画（avis）AVIF 仍不支持，
/// 由 image/mp4parse 返回明确错误。
fn decode_image(path: &str) -> Result<DynamicImage, String> {
    if is_animated_avif(path) {
        return Err("暂不支持动画 AVIF（avis），请先转换为静态图片".to_string());
    }
    let reader = image::ImageReader::open(path)
        .map_err(|e| format!("打开图片失败: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("识别图片格式失败: {}", e))?;
    let is_avif = reader.format() == Some(image::ImageFormat::Avif);
    reader.decode().map_err(|e| {
        if is_avif {
            format!("解码 AVIF 失败（可能是不支持的 grid 分块或动画 AVIF）: {}", e)
        } else {
            format!("解码图片失败: {}", e)
        }
    })
}

/// 解码 → 旋转 → 缩放/裁剪，转换与预览共用同一条管线
fn process_image(path: &str, o: &ConvertOptions) -> Result<DynamicImage, String> {
    let img = decode_image(path)?;
    let img = match o.rotation.as_str() {
        "exif" => exif_rotate(img, path),
        "cw90" => img.rotate90(),
        "ccw90" => img.rotate270(),
        "rot180" => img.rotate180(),
        _ => img,
    };
    apply_resize(img, o)
}

fn encode_image(img: DynamicImage, o: &ConvertOptions, out: &PathBuf) -> Result<(), String> {
    let q = o.quality.unwrap_or(80).clamp(1, 100);
    if o.format == "webp" {
        let enc =
            webp::Encoder::from_image(&img).map_err(|e| format!("webp 编码器创建失败: {}", e))?;
        let mem = if o.lossless { enc.encode_lossless() } else { enc.encode(q as f32) };
        return fs::write(out, &*mem).map_err(|e| format!("写入失败: {}", e));
    }
    let mut file = fs::File::create(out).map_err(|e| format!("创建文件失败: {}", e))?;
    let result = match o.format.as_str() {
        "jpg" => DynamicImage::ImageRgb8(img.into_rgb8())
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(file, q)),
        "png" => img.write_with_encoder(image::codecs::png::PngEncoder::new(file)),
        "bmp" => DynamicImage::ImageRgb8(img.into_rgb8())
            .write_with_encoder(image::codecs::bmp::BmpEncoder::new(&mut file)),
        // speed 6：质量/速度折中；ravif 无真无损，AVIF 故不提供无损选项
        "avif" => DynamicImage::ImageRgba8(img.into_rgba8())
            .write_with_encoder(image::codecs::avif::AvifEncoder::new_with_speed_quality(file, 6, q)),
        other => return Err(format!("未知格式: {}", other)),
    };
    result.map_err(|e| format!("编码失败: {}", e))
}

/// 不覆盖已有文件：hive_name.ext 冲突时 → hive_name_1.ext …
fn fresh_output_path(dir: &str, stem: &str, prefix: &str, ext: &str) -> PathBuf {
    let base = PathBuf::from(dir);
    let first = base.join(format!("{}{}.{}", prefix, stem, ext));
    if !first.exists() {
        return first;
    }
    for i in 1..1000u32 {
        let p = base.join(format!("{}{}_{}.{}", prefix, stem, i, ext));
        if !p.exists() {
            return p;
        }
    }
    first
}

/// convertSettings 允许被前端 patch 的字段（与 viewer.ts 的 ConvertSettings 一一对应）。
const CONVERT_SETTING_KEYS: &[&str] = &[
    "rotation", "resizeMode", "width", "height", "padColor", "format", "lossless", "quality",
    "outMode", "customDir", "prefix",
];

/// 把增量 patch 合并进当前 convertSettings（纯函数，便于单测）：
///  * 只认白名单字段，未知键报错（防手改文件/前端 bug 写坏设置）
///  * width/height 允许 null（表示不限制）
///  * quality 四舍五入并 clamp 到 1..=100
///
/// 非对象 patch 直接拒绝；current 不是对象时按空对象处理（旧文件/脏数据容错）。
fn merge_convert_patch(
    current: &serde_json::Value,
    patch: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let patch_obj = patch
        .as_object()
        .ok_or_else(|| "patch 必须是 JSON 对象".to_string())?;
    let mut merged = current.as_object().cloned().unwrap_or_default();
    for (key, value) in patch_obj {
        if !CONVERT_SETTING_KEYS.contains(&key.as_str()) {
            return Err(format!("未知的转换设置字段: {}", key));
        }
        let value = if key == "quality" {
            let q = value
                .as_f64()
                .ok_or_else(|| "quality 必须是数字".to_string())?;
            serde_json::json!(q.round().clamp(1.0, 100.0) as u8)
        } else {
            value.clone()
        };
        merged.insert(key.clone(), value);
    }
    Ok(serde_json::Value::Object(merged))
}

/// 原子更新共享的 convert-settings.json：读当前 convertSettings → 合并 patch → set + save，
/// 返回合并结果。这是 convertSettings 的唯一写入口（前端不再整对象 set），
/// 两个窗口并发改不同字段不会互相覆盖；同字段仍为 last-writer-wins。
#[tauri::command(async)]
fn update_convert_settings(
    app: AppHandle,
    patch: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let _guard = CONVERT_SETTINGS_LOCK
        .lock()
        .map_err(|_| "转换设置锁失败".to_string())?;
    let store = app
        .store("convert-settings.json")
        .map_err(|e| format!("打开转换设置存储失败: {}", e))?;
    let current = store
        .get("convertSettings")
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let merged = merge_convert_patch(&current, &patch)?;
    store.set("convertSettings", merged.clone());
    store
        .save()
        .map_err(|e| format!("保存转换设置失败: {}", e))?;
    Ok(merged)
}

#[tauri::command]
async fn convert_image(path: String, options: ConvertOptions, output_dir: String) -> Result<String, String> {
    // 解码/编码是纯 CPU 重活 → blocking 线程池，不占 async worker
    tauri::async_runtime::spawn_blocking(move || {
        let img = process_image(&path, &options)?;
        let stem = PathBuf::from(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".to_string());
        let out = fresh_output_path(&output_dir, &stem, &options.prefix, &options.format);
        encode_image(img, &options, &out)?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("转换任务失败: {}", e))?
}

// ---------- 预览代理（PR proxy 模式） ----------
// 全分辨率解码只做一次，缓存 ≤1200px 代理；后续预览的几何运算都在代理上跑。
// 各裁剪模式的构图都是尺度不变的，所以目标尺寸按代理比例缩小即可，画面构图与全分辨率一致。
struct ProxyEntry {
    path: String,
    src_w: u32,
    src_h: u32,
    orientation: u32,
    img: Arc<DynamicImage>,
}

// 小 LRU：最近用过的 4 张代理留在内存，来回切图免重复解码
static PROXY: Mutex<Vec<ProxyEntry>> = Mutex::new(Vec::new());
const PROXY_LIMIT: usize = 4;

fn get_proxy(path: &str) -> Result<(u32, u32, u32, Arc<DynamicImage>), String> {
    let mut guard = PROXY.lock().map_err(|_| "预览缓存锁失败".to_string())?;
    if let Some(i) = guard.iter().position(|p| p.path == path) {
        let entry = guard.remove(i);
        let res = (entry.src_w, entry.src_h, entry.orientation, entry.img.clone());
        guard.insert(0, entry);
        return Ok(res);
    }
    let img = decode_image(path)?;
    let (src_w, src_h) = (img.width(), img.height());
    let orientation = read_orientation(path);
    let proxy = Arc::new(img.resize(1200, 1200, FilterType::Triangle));
    guard.insert(
        0,
        ProxyEntry {
            path: path.to_string(),
            src_w,
            src_h,
            orientation,
            img: Arc::clone(&proxy),
        },
    );
    guard.truncate(PROXY_LIMIT);
    Ok((src_w, src_h, orientation, proxy))
}

/// 由原始尺寸 + 设置推算最终输出尺寸（±1px 舍入误差不影响展示）
fn final_dims(src_w: u32, src_h: u32, turns: u8, o: &ConvertOptions) -> (u32, u32) {
    let (w, h) = if turns % 2 == 1 { (src_h, src_w) } else { (src_w, src_h) };
    let fit = |tw: u32, th: u32| {
        let s = (tw as f32 / w as f32).min(th as f32 / h as f32);
        (
            ((w as f32) * s).round().max(1.0) as u32,
            ((h as f32) * s).round().max(1.0) as u32,
        )
    };
    match o.resize_mode.as_str() {
        "contain" => need_dims(o).map(|(tw, th)| fit(tw, th)).unwrap_or((w, h)),
        "fit-width" => match o.width {
            Some(tw) if tw > 0 => (tw, ((h as f32) * (tw as f32 / w as f32)).round().max(1.0) as u32),
            _ => (w, h),
        },
        "pad" | "crop" | "stretch" => need_dims(o).unwrap_or((w, h)),
        _ => (w, h),
    }
}

/// 在代理上跑裁剪几何：目标尺寸按代理比例缩小，构图尺度不变
fn preview_resize(img: DynamicImage, scale: f32, o: &ConvertOptions) -> Result<DynamicImage, String> {
    let f = FilterType::Triangle;
    // 目标尺寸换算到代理尺度；比代理还大的目标收进 1600px，避免把代理放大成巨图
    let sdims = |w: u32, h: u32| {
        let mut sw = ((w as f32) * scale).round().max(1.0) as u32;
        let mut sh = ((h as f32) * scale).round().max(1.0) as u32;
        if sw > 1600 || sh > 1600 {
            let s = 1600.0 / sw.max(sh) as f32;
            sw = ((sw as f32) * s).round().max(1.0) as u32;
            sh = ((sh as f32) * s).round().max(1.0) as u32;
        }
        (sw, sh)
    };
    match o.resize_mode.as_str() {
        "none" => Ok(img),
        "contain" => need_dims(o).map(|(w, h)| {
            let (w, h) = sdims(w, h);
            img.resize(w, h, f)
        }),
        "fit-width" => match o.width {
            Some(w) if w > 0 => {
                let (w, _) = sdims(w, 1);
                Ok(img.resize(w, u32::MAX, f))
            }
            _ => Err("需要有效的宽度".to_string()),
        },
        "pad" => need_dims(o).map(|(w, h)| {
            let (w, h) = sdims(w, h);
            pad_to(img, w, h, f, parse_pad_color(o.pad_color.as_deref()))
        }),
        "crop" => need_dims(o).map(|(w, h)| {
            let (w, h) = sdims(w, h);
            img.resize_to_fill(w, h, f)
        }),
        "stretch" => need_dims(o).map(|(w, h)| {
            let (w, h) = sdims(w, h);
            img.resize_exact(w, h, f)
        }),
        other => Err(format!("未知裁剪模式: {}", other)),
    }
}

static PREVIEW_SEQ: AtomicU32 = AtomicU32::new(0);

/// 本进程生成的预览临时文件，只保留最近 2 张（当前 + 上一张，给 webview 读取留余量）；
/// ponytail: 崩溃残留的文件仍交给系统清理 temp 目录
static PREVIEW_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

#[tauri::command]
async fn preview_convert(path: String, options: ConvertOptions) -> Result<PreviewResult, String> {
    // 预览也是 CPU 重活（缩放 + JPEG 编码）→ blocking 线程池
    tauri::async_runtime::spawn_blocking(move || {
        let (src_w, src_h, orientation, proxy) = get_proxy(&path)?;
        let turns = rotation_turns(&options.rotation, orientation);
        let (width, height) = final_dims(src_w, src_h, turns, &options);
        // exif 模式走完整 Orientation 校正（含镜像），其余模式只旋转
        let img = if options.rotation == "exif" {
            orientation_transform(&proxy, orientation).unwrap_or_else(|| (*proxy).clone())
        } else {
            rotate_by(&proxy, turns)
        };
        let scale = img.width() as f32 / (if turns % 2 == 1 { src_h } else { src_w }) as f32;
        let img = preview_resize(img, scale, &options)?;
        let name = format!(
            "hive_preview_{}_{}.jpg",
            std::process::id(),
            PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let out = std::env::temp_dir().join(name);
        let file = fs::File::create(&out).map_err(|e| format!("创建预览失败: {}", e))?;
        DynamicImage::ImageRgb8(img.into_rgb8())
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(file, 75))
            .map_err(|e| format!("预览编码失败: {}", e))?;
        if let Ok(mut files) = PREVIEW_FILES.lock() {
            files.push(out.clone());
            while files.len() > 2 {
                let old = files.remove(0);
                let _ = fs::remove_file(old);
            }
        }
        Ok(PreviewResult {
            path: out.to_string_lossy().to_string(),
            width,
            height,
        })
    })
    .await
    .map_err(|e| format!("预览任务失败: {}", e))?
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.as_path().map(|x| x.to_string_lossy().to_string())))
    })
    .await
    .map_err(|e| format!("选择文件夹失败: {}", e))?
}

#[tauri::command]
fn pictures_dir(app: AppHandle) -> Result<String, String> {
    app.path()
        .picture_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// 快捷键「发送到 hive-viewer」：把图片复制到 <图片文件夹>\hive-viewer（不存在则创建）。
/// 同名文件不覆盖，自动追加 _1、_2（与转换输出同一规则）；返回复制后的目标路径。
#[tauri::command(async)]
fn send_to_hive_folder(app: AppHandle, paths: Vec<String>) -> Result<Vec<String>, String> {
    let dir = app
        .path()
        .picture_dir()
        .map_err(|e| format!("无法定位图片文件夹: {}", e))?
        .join("hive-viewer");
    fs::create_dir_all(&dir).map_err(|e| format!("创建文件夹失败: {} ({})", dir.display(), e))?;
    let dir_str = dir.to_string_lossy().to_string();
    let mut copied = Vec::with_capacity(paths.len());
    for p in &paths {
        let src = Path::new(p);
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("无效的文件名: {}", p))?;
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
        let dst = fresh_output_path(&dir_str, stem, "", ext);
        fs::copy(src, &dst).map_err(|e| format!("复制失败: {} ({})", p, e))?;
        copied.push(dst.to_string_lossy().to_string());
    }
    Ok(copied)
}

fn is_image_file(path: &str) -> bool {
    has_image_extension(Path::new(path))
}

/// 是否是转换类调用（--convert / --convert-list）。
/// 与 `convert_request` 的区别：后者要求解析出至少一个有效图片路径，
/// 而 `--convert-list` 的列表文件丢失/不可读时也会是 None —— 那种情况仍必须开
/// 转换窗口报错，绝不能退化成主窗口（更不能留下一个没有窗口的进程）。
fn is_convert_invocation(args: &[String]) -> bool {
    matches!(args.get(1).map(String::as_str), Some("--convert") | Some("--convert-list"))
}

/// 解析转换类 CLI 参数：
///   --convert <路径...>       一个或多个图片路径（资源管理器单选/命令行）
///   --convert-list <文件>     右键菜单多选写入的临时列表文件，每行一个路径
/// 过滤掉非图片项；没有任何有效路径时返回 None（调用方按普通启动处理）。
fn convert_request(args: &[String]) -> Option<Vec<String>> {
    match args.get(1).map(String::as_str) {
        Some("--convert") => {
            let paths: Vec<String> =
                args[2..].iter().filter(|p| is_image_file(p)).cloned().collect();
            (!paths.is_empty()).then_some(paths)
        }
        Some("--convert-list") => {
            let paths = read_convert_list(args.get(2)?);
            (!paths.is_empty()).then_some(paths)
        }
        _ => None,
    }
}

/// 读取列表文件（每行一个路径）并做同样的图片过滤。
/// 先读后删，且只删「我们自己写的」列表文件（见 is_owned_convert_list）——
/// CLI 参数里的任意路径绝不删除，这是明确的安全边界。
fn read_convert_list(file: &str) -> Vec<String> {
    let path = PathBuf::from(file);
    let Ok(text) = fs::read_to_string(&path) else {
        // 读失败（被占用/半写）也要清掉我们自己的临时清单；foreign 路径绝不碰
        if is_owned_convert_list(&path) {
            let _ = fs::remove_file(&path);
        }
        return Vec::new();
    };
    if is_owned_convert_list(&path) {
        let _ = fs::remove_file(&path); // best-effort：删不掉就留给系统临时目录清理
    }
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
        .filter(|line| is_image_file(line))
        .map(str::to_string)
        .collect()
}

/// 是否是 shell-ext 写入的批量列表文件：位于系统临时目录、名字为 hive-convert-*.txt。
/// 用 components 比较，避免 GetTempPath 尾部分隔符差异导致误判。
fn is_owned_convert_list(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let in_temp = path
        .parent()
        .map(|p| p.components().eq(std::env::temp_dir().components()))
        .unwrap_or(false);
    in_temp && name.starts_with("hive-convert-") && name.ends_with(".txt")
}

/// 主窗口的记忆状态：普通窗口化 / 最大化 / 全屏。
/// 普通窗口化不记忆位置，下次启动居中打开。
#[derive(Clone, Copy)]
enum MainWindowState {
    Normal,
    Maximized,
    Fullscreen,
}

impl MainWindowState {
    /// settings.json 里 mainWindowState 的取值。
    fn as_str(self) -> &'static str {
        match self {
            MainWindowState::Normal => "normal",
            MainWindowState::Maximized => "maximized",
            MainWindowState::Fullscreen => "fullscreen",
        }
    }
}

/// 主窗口按需创建：tauri.conf.json 里 main 设了 create:false，
/// 启动、收到新文件、转换窗口独活时被唤起，都走这里。
/// 建窗时恢复上次的窗口状态（见 MainWindowState），避免「先窗口化、再跳全屏」的启动闪烁。
fn create_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .expect("tauri.conf.json 缺少 main 窗口配置");
    let builder = WebviewWindowBuilder::from_config(app, config)?;
    let builder = match last_main_window_state(app) {
        MainWindowState::Fullscreen => builder.fullscreen(true),
        MainWindowState::Maximized => builder.maximized(true),
        MainWindowState::Normal => builder.center(),
    };
    let window = builder.build()?;
    // 记忆窗口状态：最大化 / 还原 / 全屏切换都会触发 Resized。
    // 最小化时 is_maximized 不可信（tao 按 WM_SIZE 的 SIZE_MINIMIZED 清标志），跳过。
    let handle = app.clone();
    window.on_window_event(move |event| {
        if !matches!(event, tauri::WindowEvent::Resized(_)) {
            return;
        }
        let Some(win) = handle.get_webview_window("main") else {
            return;
        };
        if win.is_minimized().unwrap_or(false) {
            return;
        }
        let state = if win.is_fullscreen().unwrap_or(false) {
            MainWindowState::Fullscreen
        } else if win.is_maximized().unwrap_or(false) {
            MainWindowState::Maximized
        } else {
            MainWindowState::Normal
        };
        persist_main_window_state(&handle, state);
    });
    Ok(window)
}

/// 读取上次退出时的主窗口状态（键缺失 / 非法值都按普通窗口化处理）。
fn last_main_window_state(app: &AppHandle) -> MainWindowState {
    let Ok(store) = app.store("settings.json") else {
        return MainWindowState::Normal;
    };
    let raw = store.get("mainWindowState");
    match raw.as_ref().and_then(|value| value.as_str()) {
        Some("fullscreen") => MainWindowState::Fullscreen,
        Some("maximized") => MainWindowState::Maximized,
        _ => MainWindowState::Normal,
    }
}

/// 把窗口状态写入 settings.json；值没变就跳过，避免拖动缩放时反复写盘。
fn persist_main_window_state(app: &AppHandle, state: MainWindowState) {
    let Ok(store) = app.store("settings.json") else {
        return;
    };
    if let Some(value) = store.get("mainWindowState") {
        if value.as_str() == Some(state.as_str()) {
            return;
        }
    }
    store.set("mainWindowState", state.as_str());
    let _ = store.save();
}

/// 显示主窗口（不存在则创建）；带路径时把文件交给它：
/// 已开窗走 open-file 事件，新建走 INITIAL_FILE（前端挂载后自取）。
fn open_main_window(app: &AppHandle, path: Option<String>) {
    if let Some(p) = &path {
        if let Ok(mut guard) = INITIAL_FILE.lock() {
            *guard = Some(p.clone());
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Some(p) = path {
            let _ = app.emit_to("main", "open-file", p);
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else if let Ok(window) = create_main_window(app) {
        let _ = window.set_focus();
    }
}

/// 打开（或复用）独立转换窗口；主窗口保持原样，不被唤出。
/// 路径列表先写入 PENDING_CONVERT：窗口新建时前端挂载后自取，
/// 已存在时再补一个 convert-file 事件（事件早于监听时由 pending 兜底）。
/// 空列表是合法输入：表示「转换调用但没解析出路径」（如列表文件丢失），
/// 前端据此显示错误，而不是当作什么都没发生。
fn open_convert_window(app: &AppHandle, paths: Vec<String>) {
    if let Ok(mut guard) = PENDING_CONVERT.lock() {
        *guard = Some(paths.clone());
    }
    if let Some(window) = app.get_webview_window("convert") {
        let _ = app.emit_to("convert", "convert-file", paths);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "convert", WebviewUrl::default())
        .title("Hive Viewer")
        .inner_size(1200.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .build();
}

/// 把文件移入回收站（trash crate），非永久删除。
#[tauri::command(async)]
fn delete_files(paths: Vec<String>) -> Result<(), String> {
    for p in &paths {
        trash::delete(p).map_err(|e| format!("移入回收站失败: {} ({})", p, e))?;
    }
    Ok(())
}

/// 原生模态询问框（标题为应用名），返回是否点确认；kind: info(默认)/warning/error。
/// async：blocking_show 不能跑在主线程，与 open_file 的 blocking_pick_file 同模式。
#[tauri::command]
async fn ask_confirm(
    app: AppHandle,
    message: String,
    ok_label: String,
    cancel_label: String,
    kind: Option<String>,
) -> bool {
    let kind = match kind.as_deref() {
        Some("warning") => MessageDialogKind::Warning,
        Some("error") => MessageDialogKind::Error,
        _ => MessageDialogKind::Info,
    };
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(message)
            .kind(kind)
            .buttons(MessageDialogButtons::OkCancelCustom(ok_label, cancel_label))
            .blocking_show()
    })
    .await
    .unwrap_or(false)
}

/// 用系统默认浏览器打开「最新 Release」页面（更新提示点击时调用）。
/// URL 固定写死、不接受前端参数，因此没有命令注入面。
#[tauri::command]
fn open_release_page() -> Result<(), String> {
    const URL: &str = "https://github.com/aethel-tail/hive-viewer/releases/latest";
    let mut cmd = std::process::Command::new("cmd");
    // start 的第一个参数是窗口标题，留空以免把 URL 当成标题
    cmd.args(["/C", "start", "", URL]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("打开浏览器失败: {}", e))
}

/// 注册/注销 Windows 11 右键菜单 sparse 包。复用安装包里已有的 sparse-package.ps1，
/// 避免在 Rust 里重写一遍证书信任 + Add-AppxPackage。
/// 脚本需要管理员权限（证书要进 LocalMachine\TrustedPeople），未提权时它会自己弹 UAC
/// 重新启动并把子进程输出回显，所以这里能拿到真正的错误文本。
/// async：PowerShell + UAC 会阻塞数秒，与 open_file/ask_confirm 同模式。
#[tauri::command]
async fn set_shell_context_menu(enabled: bool) -> Result<(), String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("无法定位安装目录")?
        .to_path_buf();
    let script = exe_dir.join("sparse-package.ps1");
    if !script.exists() {
        return Err("未找到 sparse-package.ps1（仅安装版可用）".to_string());
    }
    let action = if enabled { "Install" } else { "Uninstall" };
    // PowerShell + UAC 会阻塞数秒 → blocking 线程池
    let out = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("powershell");
        cmd.arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&script)
            .arg("-Action")
            .arg(action)
            .arg("-InstallDir")
            .arg(&exe_dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        cmd.output()
    })
    .await
    .map_err(|e| format!("启动 PowerShell 失败: {}", e))?
    .map_err(|e| format!("启动 PowerShell 失败: {}", e))?;
    if !out.status.success() {
        // 提权分支把子进程输出转发到 stdout，所以两边都看
        let mut detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        if detail.is_empty() {
            detail = format!("退出码 {}", out.status.code().unwrap_or(-1));
        }
        return Err(format!("{} 失败: {}", action, detail));
    }
    Ok(())
}

/// 预读持久化的 generalSettings.allowMultipleInstances。
/// settings.json 由前端 tauri-plugin-store 写在本机数据目录
/// %APPDATA%\<identifier>\settings.json；identifier 必须与 tauri.conf.json 同步。
/// 读不到（首次运行/解析失败）按默认 false 处理 → 保持单实例行为。
fn allow_multiple_instances() -> bool {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return false;
    };
    let path = PathBuf::from(appdata).join("dev.hive.viewer").join("settings.json");
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("generalSettings")
                .and_then(|g| g.get("allowMultipleInstances"))
                .and_then(|b| b.as_bool())
        })
        .unwrap_or(false)
}

pub fn run() {
    let mut builder = tauri::Builder::default();
    // 设置里开了「允许多个实例」就不注册 single-instance 插件，新实例可独立启动并自理 CLI 参数。
    if !allow_multiple_instances() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 第二实例只转发参数：--convert / --convert-list 进独立转换窗口，
            // 图片路径进主窗口（按需创建）。转换请求不再唤出主窗口。
            if let Some(paths) = convert_request(&argv) {
                open_convert_window(app, paths);
                return;
            }
            if is_convert_invocation(&argv) {
                // 转换调用但列表文件丢失/不可读：开转换窗口显示错误，绝不唤起主窗口
                open_convert_window(app, Vec::new());
                return;
            }
            open_main_window(app, argv.get(1).filter(|p| is_image_file(p)).cloned());
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            // CLI 参数分派：--convert / --convert-list 只开独立转换窗口（不创建主窗口）；
            // 图片路径交给主窗口。路径经 INITIAL_FILE / PENDING_CONVERT 传递，
            // 前端挂载后主动拉取（setup 阶段 webview 还没开始监听事件）。
            let args: Vec<String> = std::env::args().collect();
            if let Some(paths) = convert_request(&args) {
                open_convert_window(app.handle(), paths);
            } else if is_convert_invocation(&args) {
                // 转换调用但列表文件丢失/不可读：开转换窗口显示错误，不创建主窗口
                open_convert_window(app.handle(), Vec::new());
            } else {
                open_main_window(app.handle(), args.get(1).filter(|f| is_image_file(f)).cloned());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_file,
            open_folder,
            open_path,
            get_initial_file,
            take_pending_convert,
            read_exif,
            image_dims,
            update_convert_settings,
            convert_image,
            preview_convert,
            pick_folder,
            pictures_dir,
            send_to_hive_folder,
            delete_files,
            ask_confirm,
            set_shell_context_menu,
            open_release_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

