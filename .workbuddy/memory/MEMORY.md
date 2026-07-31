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
