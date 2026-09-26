import ctypes
import datetime
import locale
import os
import re
import shutil
import subprocess
import tempfile

from PyQt6.QtGui import QColor, QFont, QPalette

from .constants import CUDA_MIN_DRIVER, CUDA_VARIANTS, LARGE_MODEL_MIN_GB

try:
    from darkdetect import isDark as darkdetect_is_dark
    DARKDETECT = True
except ImportError:
    darkdetect_is_dark = None
    DARKDETECT = False

WINDOWS_LOCALE_BUFFER_SIZE = 85
NVIDIA_SMI_TIMEOUT = 15


def is_admin():
    try:
        if os.name == "nt":
            return bool(ctypes.windll.shell32.IsUserAnAdmin())
        return os.geteuid() == 0
    except (AttributeError, OSError):
        return False


def _windows_locale_name():
    if os.name != "nt":
        return None
    try:
        kernel32 = ctypes.windll.kernel32
        buffer = ctypes.create_unicode_buffer(WINDOWS_LOCALE_BUFFER_SIZE)
        kernel32.GetUserDefaultLocaleName(buffer, WINDOWS_LOCALE_BUFFER_SIZE, None)
        return buffer.value or None
    except (AttributeError, OSError, ValueError):
        return None


def _posix_locale_name():
    for name in ("LC_ALL", "LC_MESSAGES", "LANG"):
        value = os.environ.get(name)
        if value and value not in ("C", "POSIX"):
            return value
    return None


def get_locale_name():
    name = _windows_locale_name() or _posix_locale_name()
    if name:
        return name
    try:
        current = locale.getlocale()[0] or ""
    except (TypeError, ValueError):
        current = ""
    return current


def get_system_language():
    tag = get_locale_name().split(".")[0].split("@")[0].replace("-", "_")
    return "zh" if tag.lower().startswith("zh") else "en"


def system_font(point_size=10, bold=False):
    font = QFont()
    font.setPointSize(point_size)
    font.setBold(bold)
    return font


DEFAULT_FONT = system_font(10)

_ARROW_DIR = None

ARROW_DARK_SVG = (
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="8" viewBox="0 0 12 8">'
    '<path d="M1 1 L6 7 L11 1" stroke="#a0a0a0" stroke-width="2" fill="none" '
    'stroke-linecap="round"/></svg>'
)

ARROW_LIGHT_SVG = (
    '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="8" viewBox="0 0 12 8">'
    '<path d="M1 1 L6 7 L11 1" stroke="#505050" stroke-width="2" fill="none" '
    'stroke-linecap="round"/></svg>'
)


def _write_icon(path, content):
    if os.path.isfile(path):
        return
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(content)


def ensure_arrow_icons():
    global _ARROW_DIR
    if _ARROW_DIR:
        return _ARROW_DIR
    target = os.path.join(tempfile.gettempdir(), "whisper_gui_icons")
    os.makedirs(target, exist_ok=True)
    _write_icon(os.path.join(target, "arrow_dark.svg"), ARROW_DARK_SVG)
    _write_icon(os.path.join(target, "arrow_light.svg"), ARROW_LIGHT_SVG)
    _ARROW_DIR = target
    return target


def _run_hidden(command, timeout):
    creation_flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    try:
        return subprocess.run(
            command,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
            creationflags=creation_flags,
        )
    except (OSError, subprocess.SubprocessError):
        return None


def find_ffmpeg():
    return shutil.which("ffmpeg")


def has_ffmpeg():
    return find_ffmpeg() is not None


def detect_driver_version():
    smi = shutil.which("nvidia-smi")
    if not smi:
        return None
    result = _run_hidden([smi, "--query-gpu=driver_version", "--format=csv,noheader"], NVIDIA_SMI_TIMEOUT)
    if not result or result.returncode != 0:
        return None
    match = re.search(r"(\d+)\.(\d+)", result.stdout or "")
    if not match:
        return None
    return int(match.group(1)), int(match.group(2))


def detect_cuda_variants():
    driver = detect_driver_version()
    if driver is None:
        return CUDA_VARIANTS
    return tuple(
        variant for variant in CUDA_VARIANTS
        if CUDA_MIN_DRIVER.get(variant, (0, 0)) <= driver
    )


def dark_stylesheet(arrow_url):
    return """                QWidget { background:#121212; color:#e0e0e0; }
                QLabel { background:transparent; }
                QPushButton {
                    background:qlineargradient(x1:0,y1:0,x2:0,y2:1,stop:0 #2f2f2f,stop:1 #252525);
                    border:1px solid #3a3a3a; border-radius:7px; padding:7px 16px;
                }
                QPushButton:hover { background:#3a3a3a; border-color:#4a4a4a; }
                QPushButton:pressed { background:#1f1f1f; }
                QPushButton:disabled { color:#606060; background:#222; border-color:#333; }
                QPushButton#primaryBtn {
                    background:qlineargradient(x1:0,y1:0,x2:0,y2:1,stop:0 #00bcd4,stop:1 #0097a7);
                    color:#ffffff; border:none; font-weight:bold;
                }
                QPushButton#primaryBtn:hover { background:#00d3e0; }
                QPushButton#primaryBtn:pressed { background:#008b9b; }
                QPushButton#primaryBtn:disabled { background:#2a5a60; color:#8f8f8f; }
                QProgressBar { border:1px solid #3a3a3a; background:#1a1a1a; border-radius:5px; text-align:center; }
                QProgressBar::chunk { background:qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #00bcd4,stop:1 #0088cc); border-radius:5px; }
                QLineEdit, QPlainTextEdit {
                    background:#1b1b1b; border:1px solid #3a3a3a; border-radius:6px; padding:5px 8px;
                }
                QLineEdit:focus, QPlainTextEdit:focus { border-color:#00bcd4; }
                QComboBox {
                    background:#2a2a2a; border:1px solid #3a3a3a; border-radius:6px; padding:5px 8px;
                }
                QComboBox:focus { border-color:#00bcd4; }
                QComboBox::drop-down { border:none; width:22px; }
                QComboBox::down-arrow { image: url("%s"); width:12px; height:8px; }
                QComboBox QAbstractItemView {
                    background:#222; border:1px solid #3a3a3a;
                    selection-background-color:#00bcd4; selection-color:#ffffff;
                }
                QSplitter::handle { background:#2a2a2a; height:4px; }
                QScrollBar:vertical { background:transparent; width:10px; margin:2px; }
                QScrollBar::handle:vertical { background:#3a3a3a; border-radius:4px; min-height:30px; }
                QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height:0; }
                QScrollBar:horizontal { background:transparent; height:10px; margin:2px; }
                QScrollBar::handle:horizontal { background:#3a3a3a; border-radius:4px; min-width:30px; }
                QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width:0; }
                QStatusBar { background:#121212; }
            """ % arrow_url


def light_stylesheet(arrow_url):
    return """                QWidget { background:#f7f7f7; color:#202020; }
                QLabel { background:transparent; }
                QPushButton {
                    background:qlineargradient(x1:0,y1:0,x2:0,y2:1,stop:0 #ffffff,stop:1 #f0f0f0);
                    border:1px solid #cccccc; border-radius:7px; padding:7px 16px;
                }
                QPushButton:hover { background:#e8f6f8; border-color:#00aacc; }
                QPushButton:pressed { background:#e0e0e0; }
                QPushButton:disabled { color:#aaaaaa; background:#f0f0f0; border-color:#dddddd; }
                QPushButton#primaryBtn {
                    background:qlineargradient(x1:0,y1:0,x2:0,y2:1,stop:0 #00bcd4,stop:1 #0097a7);
                    color:#ffffff; border:none; font-weight:bold;
                }
                QPushButton#primaryBtn:hover { background:#00d3e0; }
                QPushButton#primaryBtn:pressed { background:#008b9b; }
                QPushButton#primaryBtn:disabled { background:#8fc7cd; color:#f0f0f0; }
                QProgressBar { border:1px solid #cccccc; background:#ffffff; border-radius:5px; text-align:center; }
                QProgressBar::chunk { background:qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #00bcd4,stop:1 #0088cc); border-radius:5px; }
                QLineEdit, QPlainTextEdit {
                    background:#ffffff; border:1px solid #cccccc; border-radius:6px; padding:5px 8px;
                }
                QLineEdit:focus, QPlainTextEdit:focus { border-color:#00aacc; }
                QComboBox {
                    background:#ffffff; border:1px solid #cccccc; border-radius:6px; padding:5px 8px;
                }
                QComboBox:focus { border-color:#00aacc; }
                QComboBox::drop-down { border:none; width:22px; }
                QComboBox::down-arrow { image: url("%s"); width:12px; height:8px; }
                QComboBox QAbstractItemView {
                    background:#fff; border:1px solid #cccccc; selection-background-color:#00bcd4;
                    selection-color:#ffffff;
                }
                QSplitter::handle { background:#dddddd; height:4px; }
                QScrollBar:vertical { background:transparent; width:10px; margin:2px; }
                QScrollBar::handle:vertical { background:#cccccc; border-radius:4px; min-height:30px; }
                QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height:0; }
                QScrollBar:horizontal { background:transparent; height:10px; margin:2px; }
                QScrollBar::handle:horizontal { background:#cccccc; border-radius:4px; min-width:30px; }
                QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width:0; }
                QStatusBar { background:#f0f0f0; }
            """ % arrow_url


def dark_palette():
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, QColor(18, 18, 18))
    palette.setColor(QPalette.ColorRole.WindowText, QColor(230, 230, 230))
    palette.setColor(QPalette.ColorRole.Base, QColor(30, 30, 30))
    palette.setColor(QPalette.ColorRole.Text, QColor(230, 230, 230))
    palette.setColor(QPalette.ColorRole.Button, QColor(50, 50, 50))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor(230, 230, 230))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(0, 200, 200))
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor(255, 255, 255))
    return palette


class LogStream:
    encoding = "utf-8"
    errors = "replace"

    def __init__(self, signal, wrapped=None):
        self.signal = signal
        self.wrapped = wrapped

    def write(self, message):
        if message and message.strip():
            timestamp = datetime.datetime.now().strftime("%H:%M:%S")
            self.signal.emit("[%s] %s" % (timestamp, message.strip()))
        return len(message) if message else 0

    def writelines(self, lines):
        for line in lines:
            self.write(line)

    def flush(self):
        if self.wrapped is not None and hasattr(self.wrapped, "flush"):
            self.wrapped.flush()

    def isatty(self):
        return False

    def writable(self):
        return True

    def readable(self):
        return False

    def seekable(self):
        return False

    def fileno(self):
        raise OSError("LogStream has no file descriptor")

    def close(self):
        self.flush()


def detect_gpu():
    try:
        import torch
    except ImportError:
        return False, None, 0
    try:
        if torch.cuda.is_available():
            name = torch.cuda.get_device_name(0)
            memory = torch.cuda.get_device_properties(0).total_memory / (1024 ** 3)
            return True, name, memory
    except (AssertionError, RuntimeError):
        pass
    try:
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return True, "Apple MPS", 6
    except (AttributeError, RuntimeError):
        pass
    return False, None, 0


def recommend_model(device, vram_gb):
    if device == "cpu" or vram_gb < 2:
        return "tiny"
    if vram_gb < 4:
        return "base"
    if vram_gb < 6:
        return "small"
    if vram_gb < LARGE_MODEL_MIN_GB:
        return "medium"
    return "large"
