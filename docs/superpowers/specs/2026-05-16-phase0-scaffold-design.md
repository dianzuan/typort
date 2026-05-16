# Phase 0 项目脚手架设计

**日期**: 2026-05-16
**状态**: 已批准

---

## 概述

搭建 typort 的 Cargo workspace 基础设施，包含全部 5 个子 crate（含 Phase 3/5 的空壳）、工程化配置和端到端测试骨架。

## 决策记录

| 决策项 | 选择 | 理由 |
|--------|------|------|
| Crate 创建策略 | 全部预创建 | workspace 结构从一开始完整，避免后续结构变更 |
| OOXML 生成策略 | 纯 quick-xml 自建 | 完全控制 XML 输出，不受 docx-rs 能力边界限制 |
| Typst 集成方式 | 依赖 typst crate（库模式） | 进程内编译，直接获取 Content 内容树 |
| Typst 版本 | 锁定 0.14.2 | 当前 crates.io 最新稳定版 |
| 工作区布局 | 方案 A：标准 Workspace（crates/ 目录） | 职责隔离清晰，顶层整洁 |
| 工程化配置 | 全套 | rustfmt + clippy + .gitignore + CI + CLAUDE.md + 测试框架 |

## 工作区结构

```
typort/
├── Cargo.toml                     # [workspace] members 定义
├── CLAUDE.md                      # 项目级开发指南
├── PRD.md                         # （已有）
├── ROADMAP.md                     # （已有）
├── rustfmt.toml
├── clippy.toml
├── .gitignore
├── .github/
│   └── workflows/
│       └── ci.yml
├── crates/
│   ├── typort-cli/                # binary: CLI 入口
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── typort-core/               # lib: Content 树遍历、元素分发
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── typort-ooxml/              # lib: OOXML 文档构建（quick-xml + zip）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── typort-math/               # lib: 数学 → OMML（Phase 3 实现）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── typort-presets/            # lib: 期刊预设加载（Phase 5 实现）
│       ├── Cargo.toml
│       └── src/lib.rs
├── tests/
│   ├── fixtures/
│   │   └── hello.typ
│   └── snapshots/
└── presets/
    └── README.md
```

## Crate 依赖关系

```
typort-cli → typort-core → typort-ooxml
                          → typort-math
                          → typort-presets
```

- `typort-cli`：仅依赖 `typort-core`，负责参数解析和错误报告
- `typort-core`：协调中枢，依赖其他三个 lib crate，负责 Typst 编译和 Content 树遍历
- `typort-ooxml`：无 typort 内部依赖，独立的 OOXML XML 生成和 .docx 打包
- `typort-math`：无 typort 内部依赖，独立的数学 → OMML 转换
- `typort-presets`：无 typort 内部依赖，独立的 TOML 预设解析

## 依赖选型

| Crate | 外部依赖 | 用途 |
|-------|----------|------|
| typort-cli | `clap` 4.x | 参数解析 |
| typort-core | `typst` 0.14.2, `typst-library` 0.14.2, `typst-syntax` 0.14.2 | Typst 编译 + Content 树访问 |
| typort-ooxml | `quick-xml` 0.37.x, `zip` 2.x | XML 生成 + ZIP 打包 |
| typort-math | `quick-xml` 0.37.x | OMML XML 生成 |
| typort-presets | `toml` 0.8.x, `serde` 1.x | TOML 解析 |

共享依赖在根 `Cargo.toml` 的 `[workspace.dependencies]` 统一管理版本。

## 工程化配置

### rustfmt.toml

- `edition = "2024"`
- `max_width = 100`

### clippy.toml

- 启用 pedantic lint 组
- 允许少量常见 exception（`too_many_arguments`, `module_name_repetitions` 等）

### .gitignore

- `target/`
- `*.docx`（测试输出产物）
- IDE 文件（`.idea/`, `.vscode/`）

### CI (GitHub Actions)

- Rust: stable
- Jobs: `check`, `clippy`, `test`, `fmt`
- 缓存: `~/.cargo` 和 `target/`

## 验收标准

Phase 0 完成时必须满足：

1. `cargo build` 全部通过，无 warning
2. `cargo test` 通过一个端到端集成测试：读入 `tests/fixtures/hello.typ` → 输出 `hello.docx` → 解压验证 `[Content_Types].xml` 存在
3. `typort-core` 能调用 typst crate 编译一个 `.typ` 文件并打印 Content 树的顶层元素类型
4. CI 流水线全绿

## 已知约束

- Typst crate API 标记为不稳定，0.14.x → 0.15.x 可能有破坏性变更
- `typort-math` 和 `typort-presets` 在 Phase 0 仅为空壳（`pub fn placeholder() {}` 级别），不含实际逻辑
- 端到端测试在 Phase 0 仅验证 .docx ZIP 结构合法性，不验证内容语义
