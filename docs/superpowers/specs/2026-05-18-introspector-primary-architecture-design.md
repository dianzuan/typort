# 方案 B：Introspector 主导架构设计

## 核心原则

**Typst 原生转换，无中间件。** 直接从 Typst 编译器的语义表示（Content AST）映射到 Word XML，不经过 HTML 解析、PDF 中间格式、或任何第三方转换器。

## 数据源分工

| 数据源 | 唯一职责 | 不负责 |
|--------|---------|--------|
| **Introspector** | 元素内容（Content AST，含字段、label、numbering） | 文档顺序 |
| **HtmlDocument** | 文档顺序（Tag Start/End 序列） | 元素语义解析 |
| **PagedDocument** | 页面物理参数（page size, margins, docGrid）+ 图片 bytes | 语义结构 |

三个数据源各只负责一件事，没有交叉。

## 转换流程

```
1. typst::compile::<HtmlDocument>(world)
   → HtmlDocument { root, introspector }

2. 遍历 root 树，提取 Tag 序列（按文档顺序）
   → Vec<TagEvent> where TagEvent = Start(elem_name, Location) | End(Location)

3. 对每个 Start Tag，通过 Location 查询 Introspector
   → Content AST（HeadingElem, FigureElem, RefElem, ...）

4. Content AST → Document model（typort-ooxml::document::*）
   → 每种元素类型有对应的转换器

5. typst::compile::<PagedDocument>(world)
   → 提取 page size, margins, docGrid
   → 提取 FrameItem::Image bytes（图片原始数据）

6. Document model → write_docx() → .docx ZIP
```

## 元素转换器清单

每种 Typst 元素类型对应一个转换器，统一签名：

```rust
fn convert_XXX(content: &Content, introspector: &Introspector) -> BlockElement / InlineElement
```

| Tag name | Typst 元素 | 查询到的数据 | Word 输出 | 状态 |
|----------|-----------|-------------|-----------|------|
| `heading` | `HeadingElem` | level, body, label, numbering | `w:p` + `w:pStyle Heading{N}` | 待重构 |
| `par` | `ParElem` | children（含 Text, Emph, Strong, Space 等） | `w:p` + `w:r` runs | 待重构 |
| `strong` | `StrongElem` | body | `w:r` + `w:b` | 待重构 |
| `emph` | `EmphElem` | body | `w:r` + `w:i` | 待重构 |
| `equation` | `EquationElem` | math AST, block flag, numbering | `m:oMath` / `m:oMathPara` | **已实现** |
| `image` | `ImageElem` | source path（通过 PagedDocument 取 bytes） | `w:drawing` + `wp:inline` + `a:blip` | 待实现 |
| `figure` | `FigureElem` | body, caption, label, numbering, kind | body + caption 段落 + bookmark | 待实现 |
| `table` | `TableElem` | children（rows, cells） | `w:tbl` + `w:tr` + `w:tc` | 待重构 |
| `ref` | `RefElem` | target Label | `w:fldSimple REF` 或 `w:hyperlink` | 待实现 |
| `footnote` | `FootnoteElem` | body Content | `w:footnoteReference` + `footnotes.xml` | 待重构 |
| `list` | `ListElem` | items | `w:p` + `w:numPr`（bullet） | 待重构 |
| `enum` | `EnumElem` | items, start | `w:p` + `w:numPr`（decimal） | 待重构 |
| `terms` | `TermsElem` | children (term, description) | `w:p` bold term + indented description | 待重构 |
| `link` | — | dest, body | `w:hyperlink` | 待实现 |
| `caption` | `CaptionElem` | body, separator, numbering | 段落 "图 1: ..." | 待实现 |
| `entry` | `FootnoteEntry` | 脚注区域标记 | 跳过（内容已从 FootnoteElem 取） | — |
| `super` | — | 上标文本 | `w:vertAlign superscript` | 已实现 |

## 图片嵌入路径

图片数据不在 Introspector 里（`ImageElem.source` 只有路径），需要从 PagedDocument 取：

```
1. Introspector 查 ImageElem → 知道有图片，拿到在文档中的位置
2. PagedDocument 遍历 → FrameItem::Image(Image, Size) → Image.kind()
   → Raster: data() 返回 PNG/JPG bytes, width()/height() 返回像素尺寸
   → Svg: data() 返回 SVG bytes
3. 关联：通过 Tag Location 或图片顺序匹配
4. 嵌入：bytes → word/media/imageN.{png,jpg} + w:drawing XML
```

SVG 和 Typst 原生绘图（`FrameItem::Shape`）的处理：
- Typst 编译器已经把所有绘图渲染成 Frame 树
- 可以用 typst 的渲染器把 Frame 光栅化为 PNG
- 或者直接从 PagedDocument 的 Frame 导出 SVG（Typst 自带此能力）

## 交叉引用路径

```
1. Introspector 查 RefElem → target: Label("fig-demo")
2. Introspector 查 Label("fig-demo") → 找到 FigureElem
3. FigureElem 有 numbering → 生成 "图 1"
4. Word 输出：
   - 在 FigureElem 位置插入 w:bookmarkStart name="fig-demo"
   - 在 RefElem 位置插入 w:fldSimple REF fig-demo \h
```

## 页面级属性（从 PagedDocument 提取）

| 属性 | 来源 | Word XML |
|------|------|----------|
| 页面尺寸 | `page.frame.width()/height()` | `w:pgSz` |
| 页边距 | 内容边界推算 | `w:pgMar` |
| 文档网格 | 行距 + 字号计算 | `w:docGrid type="linesAndChars"` |
| 字体/字号 | 统计最频繁的 TextItem | `w:docDefaults` + `w:rFonts` |

## 与现有代码的关系

这是一次架构重构，不是从零开始。变化范围：

| 模块 | 变化 |
|------|------|
| `typort-core/world.rs` | **已完成** — World::file() 已支持文件读取 |
| `typort-core/convert.rs` | **重写** — 从 HTML 标签解析改为 Tag+Introspector 驱动 |
| `typort-math/` | **不变** — EquationElem → OMML 已经是方案 B 模式 |
| `typort-ooxml/document.rs` | **扩展** — 新增 Image、Bookmark、Field 等文档元素 |
| `typort-ooxml/writer.rs` | **扩展** — 新增 w:drawing、w:bookmarkStart、w:fldSimple 生成 |
| `typort-presets/` | **不变** |

## 实施顺序

1. **Phase 1：骨架重写** — 新的 Tag 遍历 + Introspector 查询框架，先让 heading/par/strong/emph 走新路径，输出与现有等价
2. **Phase 2：图片嵌入** — World::file() 已就绪，实现 w:drawing XML 生成 + 图片 bytes 嵌入
3. **Phase 3：交叉引用** — RefElem → bookmark + REF 域代码
4. **Phase 4：Figure/Caption** — 图表标题、编号、bookmark
5. **Phase 5：数学补全** — 矩阵、重音、对齐、命名函数等剩余 OMML 元素
6. **Phase 6：页面控制** — docGrid、keepNext、分节、页眉页脚

## 已知限制

- OMML 不支持数学内着色、可伸缩箭头、cancel
- Word 强制数学区域使用 Cambria Math 字体
- Typst `measure`/`layout` 依赖的内容在 HtmlDocument 中被忽略（Typst issue #7185），需要从 PagedDocument 恢复
- Typst show rules 的自定义 HTML 输出可能产生非标准 Tag 名称，需要 fallback 处理

## Spike 验证结果（2026-05-18）

9/9 测试通过，验证了：
- Introspector 能查到 HeadingElem、ImageElem、FigureElem、RefElem、FootnoteElem、EquationElem
- HtmlDocument 中 Tag 序列按文档顺序排列
- Tag Location → Introspector query 精确关联
- PagedDocument FrameItem::Image 能拿到图片原始 bytes（Raster PNG 73 bytes）
- World::file() 已支持相对路径文件读取
