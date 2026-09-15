/**
 * 高画质降采样（适配窗口/缩小时让大图不再发糊）。
 *
 * 浏览器把 <img> 的缩放交给合成器，滤波只有双线性/mipmap 级别，没有 Lanczos 档位；
 * 这里自绘一张“显示分辨率”的位图：分步减半预滤波（原生 drawImage，快而稳），
 * 最后一道用 WebGL 做 Lanczos3 收尾。任何一步失败都返回 null，调用方保持 <img>，
 * 效果退化为浏览器默认缩放（不会白屏/报错）。
 *
 * 开销：目标位图 ≈ 视口物理像素（适配窗口时与图片大小无关），一次生成、后台分帧执行。
 */

export const HQ_MAX_PIXELS = 12_000_000; // 目标位图面积上限（约 4K 屏级）
export const HQ_MIN_SHRINK = 0.7; // 显示尺寸/原图（设备像素）≤ 该值才值得重采样
const STEP_LIMIT = 1.5; // 分步减半到这个倍率后交给最后一道

/** 串行化：GL 画布/纹理是复用的，生成任务不并发。 */
let chain: Promise<unknown> = Promise.resolve();

function serialize<T>(task: () => Promise<T>): Promise<T> {
  const run = chain.then(task, task);
  chain = run.catch(() => undefined);
  return run;
}

function yieldToPaint(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve));
}

/** 缩小到目标尺寸的高画质位图；不适用（放大/几乎不缩/超预算）或失败时返回 null。 */
export function hqDownscale(
  img: HTMLImageElement,
  targetW: number,
  targetH: number,
): Promise<HTMLCanvasElement | null> {
  const sw = img.naturalWidth;
  const sh = img.naturalHeight;
  if (!img.complete || sw === 0 || sh === 0) {
    return Promise.resolve(null);
  }
  const tw = Math.max(1, Math.round(targetW));
  const th = Math.max(1, Math.round(targetH));
  if (Math.max(tw / sw, th / sh) > HQ_MIN_SHRINK || tw * th > HQ_MAX_PIXELS) {
    return Promise.resolve(null);
  }
  return serialize(() => build(img, sw, sh, tw, th));
}

function stepCanvas(src: CanvasImageSource, dw: number, dh: number): HTMLCanvasElement | null {
  const canvas = document.createElement("canvas");
  canvas.width = dw;
  canvas.height = dh;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return null;
  }
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(src, 0, 0, dw, dh);
  return canvas;
}

async function build(
  img: HTMLImageElement,
  sw: number,
  sh: number,
  tw: number,
  th: number,
): Promise<HTMLCanvasElement | null> {
  let src: CanvasImageSource = img;
  let cw = sw;
  let ch = sh;
  let stepped: HTMLCanvasElement | null = null;
  while (Math.max(cw / tw, ch / th) > STEP_LIMIT) {
    const nw = Math.max(tw, Math.round(cw / 2));
    const nh = Math.max(th, Math.round(ch / 2));
    if (nw >= cw && nh >= ch) {
      break;
    }
    const next = stepCanvas(src, nw, nh);
    if (!next) {
      return null;
    }
    if (src instanceof HTMLCanvasElement) {
      src.width = 1; // 已画出下一步：立即释放上一张中间画布
      src.height = 1;
    }
    src = next;
    cw = nw;
    ch = nh;
    stepped = next;
    await yieldToPaint();
  }
  if (stepped && cw === tw && ch === th) {
    return stepped;
  }

  const out = document.createElement("canvas");
  out.width = tw;
  out.height = th;
  const ctx = out.getContext("2d");
  if (!ctx) {
    return null;
  }
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";

  // 分步后的残余一律交给 Lanczos 收尾；未分步（轻微缩小）保持原生一步：
  // img（含 EXIF/AVIF 方向）直接进纹理时方向语义不明确，且 1x~1.5x 的差距很小。
  if (stepped) {
    const gl = getGlPipe();
    if (gl && lanczosInto(gl, stepped, tw, th)) {
      ctx.drawImage(gl.canvas, 0, 0, tw, th);
      gl.canvas.width = 1; // 结果已拷出：释放共享 GL 画布的尺寸占用（下次用前重设）
      gl.canvas.height = 1;
    } else {
      ctx.drawImage(stepped, 0, 0, tw, th);
    }
    stepped.width = 1; // 结果已拷出：释放最后一张中间画布
    stepped.height = 1;
    return out;
  }
  ctx.drawImage(src, 0, 0, tw, th);
  return out;
}

interface GlPipe {
  canvas: HTMLCanvasElement;
  gl: WebGLRenderingContext;
  program: WebGLProgram;
  texture: WebGLTexture;
  buffer: WebGLBuffer;
  aPos: number;
  uSrcSize: WebGLUniformLocation | null;
  uRatio: WebGLUniformLocation | null;
  uOutSize: WebGLUniformLocation | null;
}

let glPipe: GlPipe | null | undefined;

function getGlPipe(): GlPipe | null {
  if (glPipe !== undefined) {
    return glPipe;
  }
  glPipe = null;
  const canvas = document.createElement("canvas");
  const gl = canvas.getContext("webgl", {
    alpha: true,
    premultipliedAlpha: true,
    antialias: false,
    depth: false,
    stencil: false,
    preserveDrawingBuffer: true,
  });
  if (!gl) {
    return null;
  }
  const vs = compileShader(gl, gl.VERTEX_SHADER, VERTEX_SRC);
  const fs = compileShader(gl, gl.FRAGMENT_SHADER, FRAGMENT_SRC);
  const program = gl.createProgram();
  if (!vs || !fs || !program) {
    return null;
  }
  gl.attachShader(program, vs);
  gl.attachShader(program, fs);
  gl.bindAttribLocation(program, 0, "a_pos");
  gl.linkProgram(program);
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    return null;
  }
  const texture = gl.createTexture();
  const buffer = gl.createBuffer();
  if (!texture || !buffer) {
    return null;
  }
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]),
    gl.STATIC_DRAW,
  );
  canvas.addEventListener("webglcontextlost", (e) => {
    e.preventDefault();
    glPipe = null; // 之后一律退回原生路径
  });
  glPipe = {
    canvas,
    gl,
    program,
    texture,
    buffer,
    aPos: 0,
    uSrcSize: gl.getUniformLocation(program, "u_src_size"),
    uRatio: gl.getUniformLocation(program, "u_ratio"),
    uOutSize: gl.getUniformLocation(program, "u_out_size"),
  };
  return glPipe;
}

function compileShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string,
): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) {
    return null;
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

/** 把 src（canvas）用 Lanczos3 采样到 tw×th，画进共享 GL 画布。 */
function lanczosInto(p: GlPipe, src: HTMLCanvasElement, tw: number, th: number): boolean {
  try {
    const { gl } = p;
    const sw = src.width;
    const sh = src.height;
    p.canvas.width = tw;
    p.canvas.height = th;
    gl.getError(); // 清掉历史错误，下面只看本次
    gl.viewport(0, 0, tw, th);
    gl.useProgram(p.program);
    gl.bindTexture(gl.TEXTURE_2D, p.texture);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src);
    gl.uniform1i(gl.getUniformLocation(p.program, "u_src"), 0);
    gl.uniform2f(p.uSrcSize, sw, sh);
    gl.uniform2f(p.uRatio, sw / tw, sh / th);
    gl.uniform2f(p.uOutSize, tw, th);
    gl.bindBuffer(gl.ARRAY_BUFFER, p.buffer);
    gl.enableVertexAttribArray(p.aPos);
    gl.vertexAttribPointer(p.aPos, 2, gl.FLOAT, false, 0, 0);
    gl.disable(gl.BLEND);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
    return gl.getError() === gl.NO_ERROR;
  } catch {
    glPipe = null; // 例如画布被跨源污染（SecurityError）：之后一律走原生路径
    return false;
  }
}

const VERTEX_SRC = `
attribute vec2 a_pos;
uniform vec2 u_out_size;
varying vec2 v_px;
void main() {
  gl_Position = vec4(a_pos, 0.0, 1.0);
  v_px = vec2((a_pos.x * 0.5 + 0.5) * u_out_size.x, (0.5 - a_pos.y * 0.5) * u_out_size.y);
}
`;

// 目标像素（左上原点）→ 源图连续坐标 → 6×6 抽头 Lanczos3（沿轴可分离，权重取乘积）。
// 透明像素按预乘累加，避免透明边缘出现黑边；边缘抽头按夹取复制。
const FRAGMENT_SRC = `
precision highp float;
uniform sampler2D u_src;
uniform vec2 u_src_size;
uniform vec2 u_ratio;
varying vec2 v_px;
const float PI = 3.141592653589793;
float sinc_(float x) {
  float ax = abs(x);
  if (ax < 1e-4) return 1.0;
  return sin(PI * x) / (PI * x);
}
float lanczos3(float x) {
  float ax = abs(x);
  if (ax >= 3.0) return 0.0;
  return sinc_(x) * sinc_(x / 3.0);
}
void main() {
  vec2 src = (v_px + 0.5) * u_ratio - 0.5; // src texel-center space (top-left origin)
  vec2 base = floor(src);
  vec4 acc = vec4(0.0);
  float wsum = 0.0;
  for (int j = -2; j <= 3; j++) {
    float wy = lanczos3(src.y - (base.y + float(j)));
    for (int i = -2; i <= 3; i++) {
      float w = lanczos3(src.x - (base.x + float(i))) * wy;
      vec2 tp = clamp(base + vec2(float(i), float(j)), vec2(0.0), u_src_size - 1.0);
      vec4 c = texture2D(u_src, (tp + 0.5) / u_src_size);
      acc += vec4(c.rgb * c.a, c.a) * w;
      wsum += w;
  
    }
  }
  if (wsum <= 0.0) discard;
  gl_FragColor = acc / wsum;
}
`;
