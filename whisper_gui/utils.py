import os
import locale
import tempfile
import ctypes
import datetime

from PyQt6.QtGui import QFont, QColor, QPalette

try:
    from darkdetect import isDark as darkdetect_is_dark
    DARKDETECT = True
except ImportError:
    darkdetect_is_dark = None
    DARKDETECT = False

def is_admin():
    try:
        if os.name == 'nt':
            return ctypes.windll.shell32.IsUserAnAdmin()
        return os.geteuid() == 0
    except:
        return False

def get_system_language():
    try:
        lang, _ = locale.getdefaultlocale()
        return "zh" if (lang and lang.startswith("zh")) else "en"
    except:
        return "en"

def system_font(point_size=10, bold=False):
    f = QFont()
    f.setPointSize(point_size)
    f.setBold(bold)
    return f

DEFAULT_FONT = system_font(10)

_ARROW_DIR = None

def ensure_arrow_icons():
    global _ARROW_DIR
    if _ARROW_DIR:
        return _ARROW_DIR
    tmp = os.path.join(tempfile.gettempdir(), "whisper_gui_icons")
    os.makedirs(tmp, exist_ok=True)
    dark = os.path.join(tmp, "arrow_dark.svg")
    light = os.path.join(tmp, "arrow_light.svg")
    dark_svg = '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="8" viewBox="0 0 12 8"><path d="M1 1 L6 7 L11 1" stroke="#a0a0a0" stroke-width="2" fill="none" stroke-linecap="round"/></svg>'
    light_svg = '<svg xmlns="http://www.w3.org/2000/svg" width="12" height="8" viewBox="0 0 12 8"><path d="M1 1 L6 7 L11 1" stroke="#505050" stroke-width="2" fill="none" stroke-linecap="round"/></svg>'
    if not os.path.isfile(dark):
        with open(dark, "w", encoding="utf-8") as f:
            f.write(dark_svg)
    if not os.path.isfile(light):
        with open(light, "w", encoding="utf-8") as f:
            f.write(light_svg)
    _ARROW_DIR = tmp
    return tmp


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
                    background:#222; border:1px solid #3a3a3a; selection-background-color:#00bcd4;
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
    pal = QPalette()
    pal.setColor(QPalette.ColorRole.Window, QColor(18, 18, 18))
    pal.setColor(QPalette.ColorRole.WindowText, QColor(230, 230, 230))
    pal.setColor(QPalette.ColorRole.Base, QColor(30, 30, 30))
    pal.setColor(QPalette.ColorRole.Text, QColor(230, 230, 230))
    pal.setColor(QPalette.ColorRole.Button, QColor(50, 50, 50))
    pal.setColor(QPalette.ColorRole.ButtonText, QColor(230, 230, 230))
    pal.setColor(QPalette.ColorRole.Highlight, QColor(0, 200, 200))
    return pal



class LogStream:
    def __init__(self, signal):
        self.signal = signal
    def write(self, message):
        if message.strip():
            ts = datetime.datetime.now().strftime("%H:%M:%S")
            self.signal.emit(f"[{ts}] {message.strip()}")
    def flush(self):
        pass

def detect_gpu():
    try:
        import torch
    except ImportError:
        return False, None, 0
    if torch.cuda.is_available():
        name = torch.cuda.get_device_name(0)
        mem = torch.cuda.get_device_properties(0).total_memory / (1024**3)
        return True, name, mem
    if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
        return True, "Apple MPS", 6
    return False, None, 0

def recommend_model(device, vram_gb):
    if device == "cpu" or vram_gb < 2:
        return "tiny"
    if vram_gb < 4:
        return "base"
    if vram_gb < 6:
        return "small"
    if vram_gb < 8:
        return "medium"
    return "large"
