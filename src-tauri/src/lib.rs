use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use base64::{Engine as _, engine::general_purpose};
use image::{DynamicImage, imageops::FilterType};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

static INITIAL_FILE: Mutex<Option<String>> = Mutex::new(None);
/// 转换窗口的待处理路径：创建/通知前先写入，前端挂载后 drain，
/// 兜住「窗口已创建但 webview 尚未挂载监听」的竞态。
static PENDING_CONVERT: Mutex<Option<String>> = Mutex::new(None);

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
    use super::natural_cmp;
    use std::cmp::Ordering;

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
fn take_pending_convert() -> Option<String> {
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

/// 读取 EXIF Orientation（1=正常）
fn read_orientation(path: &str) -> u32 {
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

/// 解码 → 旋转 → 缩放/裁剪，转换与预览共用同一条管线
fn process_image(path: &str, o: &ConvertOptions) -> Result<DynamicImage, String> {
    let img = image::open(path).map_err(|e| format!("打开图片失败: {}", e))?;
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
    let img = image::open(path).map_err(|e| format!("打开图片失败: {}", e))?;
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

fn is_image_file(path: &str) -> bool {
    has_image_extension(Path::new(path))
}

/// 主窗口按需创建：tauri.conf.json 里 main 设了 create:false，
/// 启动、收到新文件、转换窗口独活时被唤起，都走这里。
fn create_main_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .expect("tauri.conf.json 缺少 main 窗口配置");
    WebviewWindowBuilder::from_config(app, config)?.build()
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
/// 路径先写入 PENDING_CONVERT：窗口新建时前端挂载后自取，
/// 已存在时再补一个 convert-file 事件（事件早于监听时由 pending 兜底）。
fn open_convert_window(app: &AppHandle, path: String) {
    if let Ok(mut guard) = PENDING_CONVERT.lock() {
        *guard = Some(path.clone());
    }
    if let Some(window) = app.get_webview_window("convert") {
        let _ = app.emit_to("convert", "convert-file", path);
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
            // 第二实例只转发参数：--convert 进独立转换窗口，图片路径进主窗口（按需创建）。
            // 转换请求不再唤出主窗口。
            if argv.len() > 2 && argv[1] == "--convert" && is_image_file(&argv[2]) {
                open_convert_window(app, argv[2].clone());
                return;
            }
            open_main_window(app, argv.get(1).filter(|p| is_image_file(p)).cloned());
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            // CLI 参数分派：--convert 只开独立转换窗口（不创建主窗口）；
            // 图片路径交给主窗口。路径经 INITIAL_FILE / PENDING_CONVERT 传递，
            // 前端挂载后主动拉取（setup 阶段 webview 还没开始监听事件）。
            let args: Vec<String> = std::env::args().collect();
            if args.len() > 2 && args[1] == "--convert" && is_image_file(&args[2]) {
                open_convert_window(app.handle(), args[2].clone());
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
            convert_image,
            preview_convert,
            pick_folder,
            pictures_dir,
            delete_files,
            ask_confirm,
            set_shell_context_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
