# Vector preview test assets

Sample SVG and Android Vector Drawable files for manually verifying the
preview added in `src/lib/vector.ts`. Open the project in the app and click each
file — it should render in the image-preview pane with a 预览/源码 toggle.

| File | Exercises |
|------|-----------|
| `sample.svg` | Plain SVG (text), rendered directly |
| `ic_heart.xml` | Basic AVD: single path, `#AARRGGBB` fill color |
| `ic_check_outline.xml` | Stroke-only path with **no** `fillColor` — must render as an outline, **not** a black blob (SVG defaults fill to black; the converter forces `fill="none"`) |
| `ic_gradient.xml` | `aapt:attr` linear gradient fill |
| `ic_group_clip.xml` | Root `alpha`, `<group>` translate/rotate/pivot transform, and `<clip-path>` |
