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

## Phase 0: 项目脚手架

**工期**: 1-2 周
**风险**: 低
**前置依赖**: 无

### 目标

搭建项目基础设施，确保后续开发在可靠的工程环境中进行。

### 关键交付物

- [ ] Cargo workspace 结构（顶层 workspace + 子 crate 划分）
- [ ] CI 流水线（GitHub Actions：cargo check / clippy / test / fmt）
- [ ] Typst 编译集成：自定义 `World` 实现，能编译 `.typ` 文件并获取 `Content` 内容树
- [ ] 端到端测试骨架：输入 `.typ` -> 输出 `.docx` -> 解压验证 XML 内容
- [ ] 最小 .docx 生成验证：能输出一个仅含“Hello World”的合法 .docx 文件

### Cargo workspace 初步规划

```
typort/
  Cargo.toml              # workspace root
  crates/
    typort-cli/           # 命令行入口
    typort-core/          # 内容树遍历 + 元素分发
    typort-ooxml/         # OOXML 文档构建
    typort-math/          # 数学公式转换 (Phase 3 启用)
    typort-presets/       # 期刊预设加载 (Phase 5 启用)
  tests/
    fixtures/             # 测试用 .typ 文件
    snapshots/            # 预期输出 XML 快照
  presets/                # 期刊预设 TOML 文件
```

### 验收标准

- `cargo build` 通过，无 warning
- `cargo test` 通过基础集成测试
- CI 流水线绿色
- 能读取任意 `.typ` 文件，输出其 Content 树的元素类型列表

---

## Phase 1: 核心文本管线

**工期**: 4-6 周
**风险**: 中
**前置依赖**: Phase 0

### 目标

打通从 Typst 编译到 .docx 输出的完整管线，实现基础文本和段落的正确转换。

### 关键交付物

- [ ] **Content 树遍历器**: 参照 `typst-html` 的 `Converter` 模式，实现元素类型分发（dispatch by element type）
- [ ] **段落/Run 映射**: `ParElem` -> `w:p`，`TextElem` -> `w:r`，保留粗体（`StrongElem`）、斜体（`EmphElem`）、删除线（`StrikeElem`）等行内格式
- [ ] **CJK 字体处理**: 生成正确的 `w:rFonts` 属性（`ascii` / `hAnsi` = Times New Roman, `eastAsia` = 宋体），生成 `fontTable.xml` 声明常用中文字体
- [ ] **页面设置**: `PageElem` 的 size/margin 映射到 `w:sectPr`/`w:pgMar`，行距映射到 `w:spacing`
- [ ] **基础样式**: 生成 `styles.xml`，定义 Normal、Heading 1-5、FootnoteText 等基础样式
- [ ] **.docx 打包**: 使用 `zip` crate 将所有 XML 部件打包为合法 .docx

### 技术要点

- 基于 `docx-rs` (bokuweb) 生成段落、run、样式、页面设置
- 使用 `quick-xml` 补充 `docx-rs` 不覆盖的 XML 片段
- 实现 `TypstWorld` trait：文件读取、字体解析、时间戳等

### 风险项

| 风险 | 缓解措施 |
|------|----------|
| Typst `Content` API 与预期不符 | 参照 `typst-html` 源码，确保遍历方式一致 |
| `docx-rs` CJK 字体 API 不完善 | 降级为 `quick-xml` 原生生成 `w:rFonts` |
| Realization 管线理解偏差 | 先做最简路径（跳过 realization，直接遍历原始 Content 树），后续再优化 |

### 验收标准

- 输入一篇纯文本中文论文（无公式、无脚注），输出的 .docx 在 Word 中：
  - 段落分割正确
  - 粗体/斜体渲染正确
  - 中文显示为宋体，英文显示为 Times New Roman
  - 页面大小为 A4，页边距符合设定值
  - 行距符合设定值

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
