# 第三方许可证声明 / Third-Party Notices

Hive Viewer 自身以 MIT 许可证发布（见 [`LICENSE`](LICENSE)）。本文件列出随发行版（可执行文件 / NSIS 安装包）一同分发的第三方组件及其许可证，以履行署名、许可证文本保留（BSD 类）与源码可获取性（MPL-2.0）等义务。

> 对应版本：Hive Viewer 1.3.0 工作树；依赖版本以 [`src-tauri/Cargo.lock`](src-tauri/Cargo.lock) 为准。依赖升级后请同步更新本文件（可用 `cargo-about` 重新生成）。

## 主要组件

| 组件                                                                                                                                                     | 版本                   | 许可证                                        | 版权                                                                                   |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- | --------------------------------------------- | -------------------------------------------------------------------------------------- |
| `re_rav1d`（Rust 版 dav1d 解码器，经 `src-tauri/vendor/dav1d-shim` 以 `dav1d` 之名供 `image` 的 `avif-native` 使用）                                     | 0.1.3                  | BSD-2-Clause                                  | Rav1d Developers, Prossimo                                                             |
| `re_rav1d::dav1d` 模块（派生自 dav1d-rs 的安全 API 封装）                                                                                                | —                      | MIT                                           | Copyright (c) 2018 Luca Barbato                                                        |
| `mp4parse`（AVIF/ISO-BMFF 容器解析：irot/imir、ispe 等）                                                                                                 | 0.17.0                 | MPL-2.0                                       | Mozilla（Ralph Giles, Matthew Gregan, Alfredo Yang, Jon Bauman, Bryce Seager van Dyk） |
| `image`                                                                                                                                                  | 0.25.10                | MIT OR Apache-2.0                             | The image-rs Developers                                                                |
| `ravif` / `rav1e` / `avif-serialize`（AVIF 编码）                                                                                                        | 0.13.0 / 0.8.1 / 0.8.9 | BSD-3-Clause / BSD-2-Clause / BSD-3-Clause    | Kornel Lesiński；Thomas Daede 等                                                       |
| `webp` / `libwebp-sys`（含内置的 libwebp C 库）                                                                                                          | 0.3.1 / 0.9.6          | MIT OR Apache-2.0；libwebp 为 BSD-3-Clause    | Jared Forth；libwebp 作者（Google Inc.）                                               |
| `kamadak-exif`                                                                                                                                           | 0.5.5                  | BSD-2-Clause                                  | KAMADA Ken'ichi                                                                        |
| `tauri`、`tauri-plugin-dialog`、`tauri-plugin-single-instance`、`tauri-plugin-store`、`wry`、`tao`、`muda`、`tray-icon`、`window-vibrancy` 等 Tauri 组件 | 2.x                    | Apache-2.0 OR MIT                             | Tauri Programme within The Commons Conservancy                                         |
| `webview2-com`、`windows`、`windows-sys`、`windows-core`                                                                                                 | 0.38 / 0.6x            | MIT OR Apache-2.0                             | Microsoft Corporation                                                                  |
| `tokio`、`tracing`、`bytes`、`mio`、`socket2`                                                                                                            | 1.x                    | MIT（部分 MIT OR Apache-2.0）                 | Tokio Contributors                                                                     |
| `serde`、`serde_json`、`thiserror`、`anyhow`、`itoa`、`ryu`、`proc-macro2`、`quote`、`syn`                                                               | 1.x / 2.x              | MIT OR Apache-2.0                             | David Tolnay, Erick Tryzelaar 等                                                       |
| `base64`                                                                                                                                                 | 0.22.1                 | MIT OR Apache-2.0                             | Marshall Pierce                                                                        |
| `trash`                                                                                                                                                  | 5.2.8                  | MIT                                           | Artur Kovacs                                                                           |
| `moxcms`                                                                                                                                                 | 0.8.1                  | BSD-3-Clause OR Apache-2.0                    | Radzivon Bartoshyk                                                                     |
| `zune-jpeg`、`zune-core`、`png`、`gif`、`qoi`、`weezl`、`tiff` 等图像编解码                                                                              | —                      | MIT OR Apache-2.0（部分含 Zlib）              | image-rs Developers、caleb 等                                                          |
| `libc`、`cc`、`cfg-if`、`log`、`bitflags`、`once_cell`、`parking_lot`、`hashbrown` 等基础库                                                              | —                      | MIT OR Apache-2.0（个别为单 MIT / Unlicense） | 各上游作者                                                                             |

完整的传递依赖清单（含每个 crate 的精确版本与校验和）见 `src-tauri/Cargo.lock`；每个 crate 的 `license` 字段可在 crates.io / 本地 `~/.cargo/registry` 中核对。

## 许可证全文与链接

### BSD-2-Clause

适用于：`re_rav1d`、`rav1e`、`kamadak-exif`、`v_frame`、`av1-grain` 等；`zerocopy` 亦可按 BSD-2-Clause 使用。

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

版权行：

- `re_rav1d`: Copyright (c) Rav1d Developers, Prossimo
- `rav1e`: Copyright (c) Thomas Daede and contributors
- `kamadak-exif`: Copyright (c) KAMADA Ken'ichi
- `v_frame`: Copyright (c) Luca Barbato

### BSD-3-Clause

适用于：`ravif`、`avif-serialize`、libwebp（`libwebp-sys` 内置的 C 库）、`brotli`（BSD-3-Clause AND MIT）、`moxcms`（亦可按 Apache-2.0 使用）等。

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

版权行：

- `ravif`、`avif-serialize`: Copyright (c) Kornel Lesiński
- libwebp: Copyright (c) Google Inc. / WebP authors
- `brotli`: Copyright (c) Daniel Reiter Horn and The Brotli Authors
- `moxcms`: Copyright (c) Radzivon Bartoshyk

### MIT

适用于：`image`、`webp`、`libwebp-sys`、`trash`、`base64`、`tokio`、`windows`/`webview2-com`、`re_rav1d::dav1d`（dav1d-rs 派生部分）等，以及本项目自身（见 [`LICENSE`](LICENSE)）。

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

版权行：

- `image`、`png`、`weezl`: Copyright (c) The image-rs Developers
- `webp`: Copyright (c) Jared Forth
- `libwebp-sys`: Copyright (c) XianYou, Kornel Lesiński
- `re_rav1d::dav1d`: Copyright (c) 2018 Luca Barbato
- `trash`: Copyright (c) Artur Kovacs
- `base64`: Copyright (c) Marshall Pierce
- `tokio`: Copyright (c) Tokio Contributors
- `webview2-com`、`windows`、`windows-sys`、`windows-core`: Copyright (c) Microsoft Corporation

### Apache-2.0

适用于（可二选一，本项目在此按 Apache-2.0 声明）：`tauri`、`tauri-plugin-dialog`、`tauri-plugin-single-instance`、`tauri-plugin-store`、`wry`、`tao`、`muda`、`tray-icon`、`window-vibrancy`（Apache-2.0 OR MIT）；`serde`、`serde_json`、`thiserror`、`anyhow`、`image`、`webp` 等（MIT OR Apache-2.0）；`moxcms`（BSD-3-Clause OR Apache-2.0）。

许可证全文：<https://www.apache.org/licenses/LICENSE-2.0>

版权行：Copyright (c) Tauri Programme within The Commons Conservancy and contributors；其余各 crate 版权归其作者所有（见上表）。

### MPL-2.0（`mp4parse`）

`mp4parse` 0.17.0 以 Mozilla Public License 2.0 发布，版权归 Mozilla 及其贡献者所有（Ralph Giles, Matthew Gregan, Alfredo Yang, Jon Bauman, Bryce Seager van Dyk 等）。

- 许可证全文：<https://mozilla.org/MPL/2.0/>
- 对应源码：<https://crates.io/crates/mp4parse/0.17.0>（版本由本仓库 `src-tauri/Cargo.lock` 锁定；`src-tauri/vendor/dav1d-shim` 只 re-export `re_rav1d`，不修改 `mp4parse`）。
- 本项目未修改 `mp4parse` 的源代码。如需要，可通过上述链接获取被覆盖源码（Covered Software）的副本。

### 其他

- `zerocopy`（BSD-2-Clause OR Apache-2.0 OR MIT）、`bytemuck`（Zlib OR Apache-2.0 OR MIT）、`zune-jpeg`/`zune-core`（MIT OR Apache-2.0 OR Zlib）、`aho-corasick`/`memchr`/`byteorder`（Unlicense OR MIT）、`to_method`（CC0-1.0）、`dunce`（CC0-1.0 OR MIT-0 OR Apache-2.0）、`unicode-ident`（MIT OR Apache-2.0 AND Unicode-3.0）等按各自 crate 元数据中的许可证使用；全文可在 crates.io 或 `~/.cargo/registry/src/` 对应目录中查阅。
