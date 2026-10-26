# GitMaster 正式品牌图像

2026-10-26，按用户批准的 H 精修黄色字标设计制作。

- 原始批准设计：[黄色 GitMaster 设计板](../h-refined/gitmaster-yellow-wordmark-v2.png)。原始候选、参考图和提示词一并保留在相邻目录。
- `logo-horizontal.png`：横向组合，用于工作台品牌入口；字体已包含在图像内，无需系统字体或网络字体。图像为白底，界面采用 multiply 混合融入浅色背景。
- `app-icon.png`：独立浅底蓝黄图标，透明外围、暖白圆角底板，用于官方 Tauri 图标生成。图标内不放文字，避免小尺寸难以辨认。

两份资源是 PNG 位图，不宣称 SVG 矢量或字体源文件。使用内置 image_gen 工具整理，应用图标以批准设计板的浅底版本为参考；保留原文件供后续矢量制作与调整。

## 桌面图标生成

使用项目锁定的 Tauri CLI：

```sh
pnpm tauri icon assets/branding/production/app-icon.png --output target/branding-check/icons
```

将生成的 32x32.png、128x128.png、128x128@2x.png、icon.icns、icon.ico 复制到 src-tauri/icons；32x32.png 同步到 public/favicon.png。仅保留当前桌面产品需要的输出，不引入移动端图标。

## 横向组合生成提示词

从已批准的参考图制作正式横向品牌图像。唯一内容是一枚蓝黄双书签标志在左，一行金黄色 GitMaster 在右。保留原图的双书签形状、蓝黄配色、顶部圆孔、V形尾部、细白缝；保留黄色GitMaster的原版圆润字体，精确拼写G大写、M大写，其余小写。图形高度为字高1.5倍。紧凑横向排版，画布3:1，四周仅5%留白，内容占90%宽度，垂直居中。
背景必须纯白 #FFFFFF，完整不透明，禁止透明通道效果、棋盘格、黑色背景。标志和文字边缘精确平滑，完全干净，无任何颗粒和杂色。只有纯色实心色块，黄色字标使用 #E9AA20，蓝色 #07386B，书签黄 #FFBC38。不要渐变、阴影、辉光、纹理、3D，不重新设计、不改变字形，不要原图顶部编号或底部示例。
