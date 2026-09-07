# Whisper GUI

本地 Whisper 语音识别工具，完全离线运行，一键管理 PyTorch 与模型

A cross-platform desktop tool for local speech transcription powered by OpenAI Whisper, with one-click PyTorch & model management.

---

## 核心特性 | Core Features

- **完全离线转录**：基于 openai-whisper 本地运行，音频无需上传，隐私安全  
  **Fully Offline**: Powered by openai-whisper, audio never leaves your computer

- **CPU / GPU 一键安装**：内置 PyTorch CPU 版与 CUDA (NVIDIA) 版检测与安装，自动卸载旧版切换，已安装版本直接提示  
  **One-click CPU / GPU Install**: Detects and installs the correct PyTorch version (CPU or CUDA), auto-switches by uninstalling the old one, and notifies you when the target version is already installed

- **模型管理**：内置模型下载器（tiny / base / small / medium / large），支持断点续传，缓存命中直接提示  
  **Model Management**: Built-in model downloader with resume support; cached models are detected and skipped

- **智能推荐**：根据设备显存自动推荐合适模型，模型下拉框显示大小  
  **Smart Recommendation**: Recommends a suitable model based on VRAM; model sizes shown in the dropdown

- **设备选择**：自动 / CPU / GPU 三档，支持 NVIDIA CUDA 与 Apple Silicon (MPS) 加速  
  **Device Selection**: Auto / CPU / GPU modes, supporting NVIDIA CUDA and Apple Silicon (MPS)

- **跨平台**：Windows / macOS / Linux 均可运行  
  **Cross-platform**: Runs on Windows / macOS / Linux

- **多语言支持**：内置中文与英文，一键切换  
  **Multi-language**: Built-in Chinese and English, switchable anytime

- **批量转录**：一次选择多个音频文件，按队列依次转录并分别保存结果  
  **Batch Transcription**: Select multiple audio files at once; they are transcribed in order and saved separately

- **自动语言检测**：默认自动识别音频语言，也可手动指定识别语言  
  **Auto Language Detection**: Detects the audio language automatically, with manual language selection available

---

## 使用说明 | Usage Guide

1. 运行 `WhisperGUI.py`（需已安装 Python 与依赖）  
   Run `WhisperGUI.py` (requires Python and dependencies)

2. 首次使用点击「安装 CPU 版本」或「安装 GPU 版本」安装 PyTorch（GPU 版需 NVIDIA 显卡）  
   On first run, click “Install CPU Version” or “Install GPU Version” to set up PyTorch (GPU version requires an NVIDIA card)

3. 点击「安装模型」选择并下载模型，已下载的模型会直接提示  
   Click “Install Model” to download a model; already-cached models are skipped

4. 点击「浏览」选择一个或多个音频文件（支持 mp3 / wav / m4a / mp4 / flac / wma 等）  
   Click “Browse” to select one or more audio files (mp3 / wav / m4a / mp4 / flac / wma, etc.)

5. 选择输出目录与模型大小（可点「推荐」自动选择）  
   Choose an output directory and model size (click “Recommend” for an automatic pick)

6. 点击「开始转录」，进度与日志实时显示，完成后结果自动保存  
   Click “Start Transcription”; progress and logs appear live, and the result is saved automatically

---

## 安装依赖 | Dependencies

- Python 3.8+
- PyTorch（CPU 或 CUDA 版，程序内可一键安装；GPU 版仅支持 NVIDIA，macOS 请使用 CPU 版或 Apple Silicon MPS）
  PyTorch (CPU or CUDA; installable from within the app. GPU version requires NVIDIA; macOS uses CPU or Apple MPS)
- openai-whisper
- PyQt6
- darkdetect（可选，用于跟随系统主题 / optional, for system theme detection）

## 项目贡献者 | Contributors

| 贡献者 (Contributor) | 贡献内容 (Contribution) |
|----------------------|--------------------------|
| Minecraft-1314 | 完整开发 (Complete development) |
| *(欢迎提交 PR 加入贡献者列表)* | *(Welcome to submit PR to join the contributor list)* |

---

## 许可协议 | License

本项目采用 MIT 许可证，详情参见 `LICENSE` 文件。  
This project is licensed under the MIT License, see the `LICENSE` file for details.

---

## 支持我们 | Support Us

如果这个项目对您有帮助，欢迎点亮右上角的 Star ⭐ 支持我们，这将是对所有贡献者最大的鼓励！  
If this project is helpful to you, please feel free to star it in the upper right corner ⭐ to support us, which will be the greatest encouragement to all contributors!
