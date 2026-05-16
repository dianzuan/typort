# typort -- 开发路线图 (ROADMAP)

**项目名称**: typort (Typst + export)
**版本**: v0.1
**日期**: 2026-05-16

---

## 总览

typort 采用分阶段递增式开发，共 6 个阶段。每个阶段产出可独立验证的交付物，后一阶段依赖前一阶段的基础设施。总预估工期 24-32 周（单人），16-20 周（双人）。

### 里程碑时间线

```
Week  0    4    6   10   16   20   24
  |----|----|----|----|----|----|----|--->
  P0   P0   P1   P2   P3   P4   P5
  脚手架 |  核心文本  |  文档结构  |  数学公式  |  文献元数据  |  预设与打磨
       |         |         |         |         |         |
       v         v         v         v         v         v
    CI/测试   基础.docx  完整论文   公式可编辑  引文完整   期刊验收
    就绪      能打开     结构完整   非图片     格式正确   可投稿
```

### 阶段依赖关系

```
Phase 0 (脚手架)
   |
   v
Phase 1 (核心文本管线) ----+
   |                       |
   v                       v
Phase 2 (文档结构)    Phase 3 (数学公式)  [可并行]
   |                       |
   +-------+-------+-------+
           |
           v
      Phase 4 (文献与元数据)
           |
           v
      Phase 5 (期刊预设与打磨)
```

> Phase 2 与 Phase 3 无直接依赖，可由两人并行推进。

---

## Phase 0: 项目脚手架 ✅ 已完成 (2026-05-16)

**工期**: 1 天（原估 1-2 周）
**状态**: 已完成

### 交付成果

- [x] Cargo workspace 结构（5 crates under `crates/`）
- [x] CI 流水线（GitHub Actions：check / clippy / test / fmt）
- [x] Typst 编译集成：`TyportWorld` 实现 `typst::World` trait
- [x] 端到端管线：`.typ` → 编译 → 文本提取 → `.docx` ZIP
- [x] 最小 .docx 生成：合法 ZIP + 完整 OOXML 部件
- [x] 5 个测试通过，clippy 无 warning

### 🔴 架构决策（经三轮调研确定）

Phase 0 暴露致命问题：`PagedDocument` 帧遍历 = PDF→Word（语义全丢）。
进一步研究发现 `HtmlDocument` 中转也有致命缺陷：数学变 SVG、格式信息丢失、编号 scheme 丢失。

**最终决策：直接调用 `typst_realize::realize`，拿 `Vec<Pair>` 做元素分发。**

这是 PRD 原始设想的"自定义 Realization Pass"的正确实现：
- `typst_realize::realize` 是 pub API，可从外部调用
- 返回 `Vec<(&Content, StyleChain)>` —— 已展开 show rules 的元素列表
- 每个元素可用 `content.to_packed::<HeadingElem>()` 做类型分发
- **StyleChain 携带完整格式信息**（字体、字号、行距、缩进 —— 精确到 pt）
- **EquationElem 在 realize 后原样保留**（数学不经过任何 show rule），`.body` 是完整语义树
- **FootnoteElem 原始结构可用**

**后备策略**：若 Typst 上游对 `typst_realize` 做破坏性变更堵死入口，我们自建 Content 树一一映射（硬遍历所有 Element 类型，自行展开 show rules）。

---

## Phase 1: 直通语义管线（Realize → OOXML）

**工期**: 3-4 周
**风险**: 中
**前置依赖**: Phase 0 ✅

### 目标

搭建从 Content 树直接到 OOXML 的语义管线，不经过 HTML 或 PDF 任何中间格式。实现段落、标题、格式、字体的正确转换。

### 核心架构

```
.typ 源文件
    │
    ▼
typst eval → Content 内容树
    │
    ▼
typst_realize::realize(RealizationKind::HtmlDocument, ...)
    │
    ▼
Vec<Pair> = Vec<(&Content, StyleChain)>  ← 已展开 show rules
    │
    ▼
元素分发器 (to_packed::<HeadingElem>, <ParElem>, <TextElem>, ...)
    │
    ├── HeadingElem + StyleChain → w:p + Heading样式 + numbering
    ├── ParElem → w:p
    ├── TextElem + StyleChain → w:r + w:rPr（字体/字号/粗斜体从 StyleChain 精确读取）
    ├── EquationElem.body → OMML (m:oMath)  ← 完整语义数学树
    ├── FootnoteElem → w:footnoteReference + footnotes.xml
    ├── TableElem → w:tbl
    ├── ListElem/EnumElem → w:p + w:numPr
    └── StrongElem/EmphElem → w:b / w:i
    │
    ▼
typort-ooxml (quick-xml) → ZIP → .docx
```

### 关键技术步骤

1. **接入 Realize API**
   - 添加 `typst-realize`、`typst-html`（用其 `RealizationKind`）依赖
   - 启用 `Feature::Html` 使 realize 按 HTML target 展开
   - 构建 Engine（需 Routines、Introspector、Traced、Sink）
   - 调用 `(engine.routines.realize)(kind, &mut engine, ...)` 获取 `Vec<Pair>`

2. **元素分发器**（替换当前 `convert.rs`）
   - 遍历 `Vec<Pair>`，对每个 `(content, styles)` 调用 `to_packed::<T>()`
   - 从 `StyleChain` 读取精确格式值：
     - `TextElem::font_in(styles)` → 字体名
     - `TextElem::size_in(styles)` → Abs 字号
     - `TextElem::weight_in(styles)` → 粗细
     - `ParElem::leading_in(styles)` → 行距
     - `ParElem::first_line_indent_in(styles)` → 首行缩进

3. **OOXML 文档模型扩展**
   - `typort-ooxml` 的 Document 模型增加：标题级别、Run 格式属性、样式引用
   - `writer.rs` 增加 `styles.xml`、`fontTable.xml` 生成
   - `w:rPr` 支持：`w:rFonts`、`w:sz`、`w:b`、`w:i`

4. **CJK 字体 & 页面设置**
   - `w:rFonts eastAsia="宋体" ascii="Times New Roman"` hAnsi="Times New Roman"`
   - 页面设置从 Typst `#set page()` 的 Document metadata 读取
   - `w:sectPr` / `w:pgMar` / `w:spacing`

### 关键交付物

- [ ] Realize API 接入：能调用 realize 并打印 Vec<Pair> 的元素类型列表
- [ ] 元素分发器：HeadingElem、ParElem、TextElem、StrongElem、EmphElem
- [ ] StyleChain 读取：精确字体、字号、粗细
- [ ] OOXML 模型扩展：标题样式、Run 格式属性
- [ ] styles.xml + fontTable.xml 生成
- [ ] 页面设置：sectPr / pgMar
- [ ] complex_paper.typ 通过验收测试

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| `typst_realize` API 不稳定 | 锁定 0.14.2；在 typort-core 中用适配器 trait 隔离上游接口 |
| Engine 构建需要多个内部组件 | 参照 `typst-html/src/document.rs` 的 79-90 行 |
| Typst 上游删除/重命名 realize | 后备：自建 Content 树一一映射 |
| RealizationKind 未来可能不接受 HtmlDocument | 后备：使用 PagedDocument + Introspector 查询语义元素 |

### 验收标准

- 输入 `tests/fixtures/complex_paper.typ`，输出 .docx：
  - Word 导航窗格正确显示标题层级（Heading 1-5 样式）
  - 粗体/斜体渲染正确（从 StyleChain 读取）
  - 段落正确分割（每个 ParElem 独立段落）
  - 中文宋体 + 英文 Times New Roman（w:rFonts eastAsia）
  - 页面 A4，行距 1.5 倍
  - 至少 5 项结构检查通过（对比 Phase 0 的 0/14）

---

## Phase 2: 文档结构

**工期**: 4-6 周
**风险**: 中
**前置依赖**: Phase 1

### 目标

实现学术论文的核心结构化元素：标题层级、脚注、列表、表格、图片。

### 关键交付物

- [ ] **标题与中文编号**: `HeadingElem` 映射为 Word Heading 样式；通过 `numbering.xml` 定义五级中文编号（一、/ (一) / 1. / (1) / ①），使用 `chineseCountingThousand` 和 `decimalEnclosedCircleChinese` 编号格式
- [ ] **脚注**: `FootnoteElem` 映射为 `w:footnoteReference` + `footnotes.xml` 条目；在 `settings.xml` 中设置 `w:footnotePr/w:numRestart val="eachPage"` 实现逐页重编号，编号样式为 ① ② ③
- [ ] **列表**: `ListElem` 映射为项目符号列表，`EnumElem` 映射为编号列表
- [ ] **表格**: `TableElem` 映射为 `w:tbl`，支持单元格内容、边框、基本合并
- [ ] **图片**: `FigureElem` 中的图片元素提取嵌入到 `word/media/`，通过 `w:drawing` 引用；图注映射为 Caption 样式段落

### 技术要点

- 中文编号需自定义 `w:abstractNum` 定义，`docx-rs` 提供 `AbstractNumbering` / `Level` API 可部分使用
- 逐页脚注重编号属于 section-level 属性，`docx-rs` 可能不直接支持，需 `quick-xml` 补充
- 表格合并（`rowspan`/`colspan`）需映射为 OOXML 的 `w:vMerge`/`w:gridSpan`

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| 逐页脚注重编号在 `docx-rs` 中无 API | 用 `quick-xml` 直接生成 `settings.xml` 中的 `w:footnotePr` |
| 中文编号格式 `chineseCountingThousand` 在 WPS 中表现不一致 | 测试 WPS 兼容性，必要时改用 `lvlText` 硬编码 Unicode 字符 |
| 复杂表格（嵌套、多级合并）转换困难 | MVP 仅支持简单表格，复杂表格标记为已知限制 |

### 验收标准

- 输入含标题（三级以上）、脚注（每页多个）、表格、图片的中文论文，输出 .docx：
  - Word 导航窗格正确显示标题层级
  - 标题编号为中文格式（一、(一) 等）
  - 脚注为 Word 原生脚注，每页从 ① 重新编号
  - 表格结构完整，可在 Word 中编辑
  - 图片正常显示，图注编号正确

---

## Phase 3: 数学公式

**工期**: 6-8 周
**风险**: **高**（本项目最高风险阶段）
**前置依赖**: Phase 1（与 Phase 2 可并行）

### 目标

将 Typst 数学内容树直接映射为 OMML (Office Math Markup Language)，使公式在 Word 中可编辑。

### 关键交付物

- [ ] **数学内容树遍历器** (`typort-math` crate): 递归遍历 `EquationElem` 内容树，识别 30+ 数学元素类型
- [ ] **Typst-to-OMML 映射表**: 实现以下核心映射：

| Typst 元素 | OMML 元素 | 复杂度 |
|-----------|----------|--------|
| `EquationElem` (行内) | `m:oMath` | 低 |
| `EquationElem` (行间) | `m:oMathPara` > `m:oMath` | 低 |
| `FracElem` | `m:f` | 低 |
| `RootElem` | `m:rad` | 低 |
| `AttachElem` (上/下标) | `m:sSub`/`m:sSup`/`m:sSubSup`/`m:sPre` | 中 |
| `MatElem`/`VecElem` | `m:m` | 中 |
| `LrElem` (定界符) | `m:d` | 中 |
| `AccentElem` | `m:acc` | 中 |
| `OpElem` + limits | `m:nary` | 高 |
| `CasesElem` | `m:d` + `m:eqArr` | 高 |
| 数学文本 | `m:r` + `m:rPr` | 中 |

- [ ] **OMML 序列化**: 使用 `ooxml-omml` crate 或 `quick-xml` 生成 OMML XML
- [ ] **行间公式编号**: 行间公式编号（如“(1)”）映射为 OMML 段落属性或右对齐制表位
- [ ] **降级策略**: 对 OMML 无法表达的结构（如 `CancelElem`），回退为图片嵌入

### 技术要点

- 采用 Option A（直接映射）策略：Typst 数学 Content -> OMML，无中间格式
- 数学文本需区分斜体（变量）、正体（函数名如 sin, cos）、粗体（向量），映射到 `m:rPr` 的 `m:sty` 属性
- `ooxml-omml` crate (v0.1.0) 提供序列化能力但尚不成熟，需评估是否直接用 `quick-xml`

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| 30+ 元素类型工作量大 | 按使用频率排序，先实现分式/上下标/根号/矩阵等高频元素 |
| Typst 数学语义与 OMML 不完全对应 | 逐元素编写映射规则，记录不可映射的情况 |
| `CancelElem`（删除线）在 OMML 中无对应 | 回退为 `m:borderBox` 近似或图片嵌入 |
| 彩色公式在 OMML 中支持有限 | 忽略颜色属性，文档化为已知限制 |
| 多行对齐方程 (`AlignPointElem`) 语义差异 | 映射为 `m:eqArr`，接受对齐位置可能微调 |
| `ooxml-omml` 太新，可能有 bug | 准备 `quick-xml` 后备方案 |

### 验收标准

- 输入含以下数学结构的论文，输出 .docx 中公式为可编辑 OMML：
  - 行内公式与行间公式
  - 分式、根号、上下标
  - 求和/积分符号（带上下限）
  - 矩阵
  - 定界符（括号、方括号、大括号）
  - 带编号的行间公式（如“(1)”）
- 在 Word 中双击公式可进入编辑模式

---

## Phase 4: 文献与元数据

**工期**: 3-4 周
**风险**: 中
**前置依赖**: Phase 2

### 目标

完成学术论文的引文、参考文献列表和首页元数据的转换。

### 关键交付物

- [ ] **引文提取**: 从 Typst 编译后的内容树中提取 `CiteElem` 的已渲染文本（如 “[1]” “(张三, 2020)”），作为格式化文本插入 Word
- [ ] **参考文献列表**: 提取编译后的 `BibliographyElem` 渲染结果，映射为带缩进的参考文献段落，应用专用样式
- [ ] **GB/T 7714-2015 验证**: 确保 Typst 使用 GB/T 7714-2015 CSL 样式渲染的文献在转换后格式正确
- [ ] **首页脚注元数据**: 支持常见社科期刊的首页脚注字段：
  - 基金项目（如“国家社科基金重大项目(21&ZD001)”）
  - 作者简介（姓名、单位、职称、研究方向）
  - 收稿日期
  - 中图分类号 / 文献标识码 / 文章编号
- [ ] **文档属性**: 将标题、作者、关键词写入 .docx 的 `docProps/core.xml`

### 技术要点

- MVP 采用 Option A 策略：直接提取 Typst 已渲染的引文文本，不重新运行 CSL 处理器
- 首页脚注元数据通常在 Typst 中通过 `#set document()` 或自定义函数传入，需约定提取方式（如 `MetadataElem`）
- Hayagriva 和 citationberg 作为后续增强的备选（生成 Word 原生 CITATION 域代码）

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| 编译后引文文本提取位置不确定 | 研究 `typst-html` 对引文的处理方式 |
| 首页脚注元数据在 Typst 中无统一约定 | 定义 typort 专用的 metadata 传入规范（或约定命名的 `#metadata()` 调用） |
| GB/T 7714-2015 在 Typst 中的已知 bug | 关注 typst#2548 状态，必要时提供 workaround |

### 验收标准

- 输入含 `.bib` 文件和 `@cite` 引用的中文论文，输出 .docx：
  - 文中引用标注格式正确（如 [1] 或 (张三, 2020)）
  - 文末参考文献列表完整，格式符合 GB/T 7714-2015
  - 首页脚注包含基金项目和作者简介
  - Word 文档属性中包含标题和作者

---

## Phase 5: 期刊预设与打磨

**工期**: 4-6 周
**风险**: 低-中
**前置依赖**: Phase 2, Phase 3, Phase 4

### 目标

实现期刊预设配置系统，完成交叉引用、目录、页眉页脚等收尾功能，通过真实期刊模板验收测试。

### 关键交付物

- [ ] **期刊预设系统** (`typort-presets` crate):
  - TOML 格式配置文件，定义：字体族与字号、页面尺寸与页边距、行距、标题编号方案、脚注样式、参考文献格式
  - 命令行参数 `--preset <期刊名>` 加载预设
  - 内建至少 3 个期刊预设（如《管理世界》《中国社会科学》《法学研究》）

预设文件示例：
```toml
[journal]
name = "管理世界"
issn = "1002-5502"

[page]
size = "A4"
margin_top = "2.54cm"
margin_bottom = "2.54cm"
margin_left = "3.17cm"
margin_right = "3.17cm"

[font]
body_cjk = "宋体"
body_latin = "Times New Roman"
body_size = "10.5pt"          # 五号
heading1_cjk = "黑体"
heading1_size = "15pt"        # 小三号
footnote_size = "9pt"         # 小五号

[numbering]
scheme = "chinese_social_science"  # 一、(一) 1. (1) ①

[footnote]
restart = "each_page"
format = "circled_number"     # ① ② ③

[bibliography]
style = "gb-t-7714-2015-numeric"
```

- [ ] **交叉引用**: `RefElem` 映射为 Word 交叉引用文本（如“图 1”“表 2”“式(3)”）
- [ ] **目录**: `OutlineElem` 映射为 Word TOC 域（`w:sdt` + `w:fldChar` + `HYPERLINK`），用户在 Word 中按 F9 可更新
- [ ] **页眉页脚**: 根据期刊预设生成 `header1.xml`（期刊名、卷期号）和 `footer1.xml`（页码）
- [ ] **真实期刊验收测试**: 选取 3+ 期刊的真实论文，用 Typst 重写后经 typort 转换，与期刊 Word 模板对比验证
- [ ] **CLI 打磨**: 错误信息友好化、进度提示、`--verbose` 模式、`--check` 模式（仅检查不输出）

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| 期刊版式要求细节繁杂 | 先覆盖 3 个代表性期刊，逐步扩展 |
| 交叉引用依赖 Typst 内省系统 | 研究 Content 树中 `TagElem`/`CounterDisplayElem` 的可用信息 |
| 目录域代码在不同 Word 版本中行为不一致 | 生成标准 TOC 域，标注“打开后按 F9 更新” |

### 验收标准

- `typort input.typ -o output.docx --preset 管理世界` 输出的 .docx 文件：
  - 版式与《管理世界》投稿要求一致（字体、字号、行距、页边距）
  - 标题编号为中文格式
  - 脚注逐页重编号
  - 参考文献格式符合 GB/T 7714-2015
  - 在 Word 2019 和 Word 365 中渲染一致
- 至少通过 3 个不同期刊的版式验收

---

## 整体风险矩阵

| 风险 | 严重性 | 可能性 | 涉及阶段 | 缓解策略 |
|------|--------|--------|----------|----------|
| Typst crate API 版本更新破坏兼容 | 高 | 极高 | 全部 | 锁定版本，适配器 trait 隔离，预留维护工时 |
| 数学转换不完整/不准确 | 高 | 高 | Phase 3 | 按频率优先实现，图片降级兜底 |
| docx-rs 功能缺口 | 中 | 高 | Phase 1-2 | 从设计之初就预留 quick-xml 原生 XML 通道 |
| OMML 在不同 Word 版本中渲染差异 | 中 | 中 | Phase 3 | 多版本测试矩阵（Word 2019/365/WPS） |
| 内容树未暴露所有所需信息 | 高 | 中 | Phase 1-4 | 参照 typst-html 的处理方式，必要时使用内省 API |
| CJK 字体嵌入/替换问题 | 中 | 中 | Phase 1 | 测试常用字体集，文档化字体安装要求 |
| 期刊版式要求复杂多变 | 中 | 高 | Phase 5 | TOML 配置外部化，社区众包预设 |

---

## 关键指标

| 指标 | 目标值 | 衡量时点 |
|------|--------|----------|
| 基础文本转换正确率 | >= 95% | Phase 1 完成 |
| 结构元素转换正确率（标题/脚注/表格） | >= 90% | Phase 2 完成 |
| 常用数学公式转换正确率 | >= 85% | Phase 3 完成 |
| 期刊模板验收通过率 | >= 3 个期刊 | Phase 5 完成 |
| 端到端转换时间（普通论文） | < 10 秒 | Phase 5 完成 |
| Word 2019/365/WPS 兼容率 | >= 95% | Phase 5 完成 |
