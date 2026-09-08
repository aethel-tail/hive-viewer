// .stylelintrc.mjs

/** @type {import('stylelint').Config} */
export default {
  // 继承我们推荐的、集成了 Vue 支持的标准化配置
  extends: [
    "stylelint-config-standard-vue", // 替换了 stylelint-config-standard，并内置了 Vue SFC 解析
    "stylelint-config-recess-order", // CSS 属性排序规则
    // "prettier" 已被移除，因为新版 stylelint 不再需要它来解决冲突
  ],

  // 注册需要使用的插件
  plugins: [
    "stylelint-order", // stylelint-config-recess-order 的核心插件
    "stylelint-declaration-block-no-ignored-properties", // 检查被忽略属性的插件
    // "stylelint-config-rational-order/plugin" 是多余的，已移除
  ],

  // 你的自定义规则（全部保留并优化）
  rules: {
    // 启用插件规则
    "plugin/declaration-block-no-ignored-properties": true,

    // 禁用导致崩溃的规则
    "declaration-block-no-redundant-longhand-properties": null,

    // --- 以下是你自己的规则，我们予以保留 ---
    "comment-empty-line-before": null,
    "declaration-empty-line-before": null,
    "function-name-case": "lower",
    "no-descending-specificity": null,
    "no-invalid-double-slash-comments": null,
    "rule-empty-line-before": ["always", { except: ["first-nested"] }],

    // 你的 BEM 风格命名规则，非常好，予以保留。
    // 如果在 Vue 的 scoped style 中使用 :deep() 等选择器时遇到问题，可以考虑放宽此规则或使用 "selector-pseudo-class-no-unknown"
    "selector-class-pattern":
      "^(el-)?[a-z][a-z0-9]*(?:-[a-z0-9]+)*(?:__[a-z0-9]+(?:-[a-z0-9]+)*)?(?:--[a-z0-9]+(?:-[a-z0-9]+)*)?$",

    // 针对 Vue 添加一些有用的规则豁免
    "selector-pseudo-class-no-unknown": [
      true,
      {
        // 允许使用 Vue 的 :deep, :slotted, :global 伪类
        ignorePseudoClasses: ["deep", "slotted", "global"],
      },
    ],
  },

  // 忽略检查的文件或文件夹，增强了通用性
  ignoreFiles: ["node_modules/**/*", "dist/**/*", "build/**/*", "public/**/*", "*.js", "*.ts"],
};
