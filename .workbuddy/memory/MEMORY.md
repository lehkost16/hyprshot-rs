# hyshot 项目长期记忆

## 项目概述
hyshot 是一个 Wayland (wlroots) 下的屏幕截图/录屏工具，使用 Rust 编写。
- 录屏功能基于 wf-recorder
- 截图区域选择基于 slurp
- 配置文件: `~/.config/hyshot/config.toml` (TOML 格式)
- 代码模块: src/config.rs (配置定义), src/config_cmds.rs (配置命令), src/record/mod.rs (录屏逻辑)

## 技术细节
- wf-recorder 根据文件扩展名自动选择封装格式 (muxer)
- 系统已安装 libx264 和 libvpx-vp9 编码器
- RecordConfig 支持配置: fps, crf, save_dir, codec, format, command_args
- format 字段控制输出文件扩展名 (webm/mp4/mkv 等)
- CRF 参数对 libvpx-vp9 (0-63) 和 libx264 (0-51) 均生效

## ocrflow 子项目
- 路径: `/home/nana/Projects/personal/ocrflow`
- 基于 ocr-rs (rust-paddle-ocr) 的全平台批量 OCR 识别工具
- 技术栈: Rust + Axum + ocr-rs (PaddleOCR + MNN) + 内嵌 Web UI
- 功能: 批量图片上传、并行 OCR 识别、AI 优化 (OpenAI 兼容 API)、CLI + Web 双模式
- 模型: PP-OCRv6 small (det + rec + keys)，存放在 `models/` 目录
- 模型来源: PP-OCRv6 模型在 rust-paddle-ocr 仓库 `next` 分支 models/ 目录；PP-OCRv5 在 `main` 分支
- ocr-rs crate 版本: 2.3.x (crates.io)
- 二进制大小: ~13MB (release, 含内嵌 Web UI)
- 支持中英文等多语言 OCR，测试平均置信度 >99%
