# WhisperGUI 0.0.1

## 重要公告（必读）

- 安装程序尚未使用代码签名证书，Windows SmartScreen 可能显示安全提示。
- 所有版本都需要 Microsoft Edge WebView2 Runtime。
- 转录功能需要 Python 3、PyTorch、openai-whisper；应用会在用户数据目录创建独立虚拟环境，不修改系统 Python。
- 非 WAV 音频需要 ffmpeg；缺少 ffmpeg 时应用会提示并允许 WAV 文件继续使用。
- 请勿在问题反馈、日志或截图中粘贴完整凭据内容。

## 项目介绍

这是一个面向 Windows 的本地语音识别工具。程序采用 **Rust 原生外壳 + WebView2 前端 + Rust 核心库 + 静态前端资源** 的分层结构，支持本地批量转录、CPU/GPU 设备选择、Whisper 模型管理、断点续传、SHA256 校验、自动语言检测、ffmpeg 预检、实时进度与日志、中英文切换以及深色和浅色主题。

音频、模型缓存和转写结果均保存在本机，运行时不请求 CDN。导入服务配置使用名称、地址、类型、凭据内容和用户 ID 五个独立字段，凭据使用 Windows DPAPI 加密。

## 下载与安装

| 产物 | 用途 | 运行条件 |
|------|------|------|
| `WhisperGUI-Setup.exe` | Inno Setup 标准安装向导 | Windows 10/11 x64，安装时需要管理员权限 |
| `WhisperGUI.msi` | 标准 Windows Installer 安装包 | Windows 10/11 x64，安装时需要管理员权限 |
| `WhisperGUI-portable-win-x64.zip` | 便携版压缩包，解压即用 | Windows 10/11 x64，需要 WebView2 Runtime |

推荐普通用户使用 `WhisperGUI-Setup.exe`。

## 许可协议

本项目采用 MIT 许可证，详情参见 [LICENSE](LICENSE) 文件。

## 支持我们

如果这个项目对您有帮助，欢迎点亮 GitHub 仓库右上角的 Star。
