import os
import sys
import datetime
import platform
import subprocess

from .bootstrap import ensure_gui_dependency, get_python_command

ensure_gui_dependency()

from PyQt6.QtCore import Qt, QSettings
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QLineEdit, QPushButton, QProgressBar, QPlainTextEdit,
    QComboBox, QFileDialog, QSplitter, QStatusBar, QStyleFactory, QMessageBox,
    QInputDialog
)
from .constants import DEVICE_CHOICES, MODEL_CACHE, MODEL_REFERENCE_SIZES_MB, SUPPORTED_LANGUAGES, WHISPER_MODELS
from .download import get_whisper_cache_dir
from .i18n import I18n
from .threads import InstallThread, ModelDownloadThread, TranscriptionThread
from .utils import (
    DARKDETECT, DEFAULT_FONT, dark_palette, dark_stylesheet, darkdetect_is_dark, detect_gpu,
    ensure_arrow_icons, get_system_language, is_admin, light_stylesheet, recommend_model, system_font
)


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.settings = QSettings("WhisperGUI", "UserPrefs")
        self.lang = self.settings.value("lang", get_system_language())
        dark_default = DARKDETECT and darkdetect_is_dark()
        self.theme_dark = self.settings.value("dark", dark_default, bool)

        has_cuda, gpu_name, vram = detect_gpu()
        self.gpu_available = has_cuda
        self.gpu_name = gpu_name
        self.vram = vram

        self.user_device_choice = self.settings.value("device", "auto")
        if self.user_device_choice not in DEVICE_CHOICES:
            self.user_device_choice = "auto"

        self.effective_device = self._resolve_effective_device()
        self.recommended_model = recommend_model(self.effective_device, self.vram if self.gpu_available else 0)
        self.output_dir = self.settings.value("output_dir", os.path.expanduser("~"))
        self.thread = None
        self.file_paths = []
        self.last_error = None
        self.manual_stop = False

        self.init_ui()
        self.apply_theme(self.theme_dark)
        self.update_texts()
        self.restore_state()

        self._log_event(I18n.get("log_app_started", self.lang))
        if not is_admin():
            self._append_log(f"[!] {I18n.get('admin_warning', self.lang)}")
        if self.gpu_available:
            self._log_event(I18n.get("gpu_detected", self.lang, name=self.gpu_name, mem=self.vram))
        else:
            self._log_event(I18n.get("no_gpu", self.lang))
        self._log_event(I18n.get("ready", self.lang))
        self.status_bar.showMessage(I18n.get("ready", self.lang))

    def _append_log(self, msg):
        self.log_view.appendPlainText(msg)
        self.log_view.verticalScrollBar().setValue(self.log_view.verticalScrollBar().maximum())

    def _log_event(self, msg):
        self._append_log(f"[{datetime.datetime.now().strftime('%H:%M:%S')}] {msg}")

    def _resolve_effective_device(self):
        if self.user_device_choice == "cpu" or not self.gpu_available:
            return "cpu"
        try:
            import torch
        except ImportError:
            return "cpu"
        if torch.cuda.is_available():
            return "cuda"
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return "mps"
        return "cpu"

    def get_effective_device(self):
        return self.effective_device

    def _device_display_name(self, device):
        if device == "cuda":
            return I18n.get("device_gpu", self.lang)
        if device == "mps":
            return "MPS"
        return I18n.get("device_cpu", self.lang)

    def init_ui(self):
        self.setWindowTitle("Whisper Speech Recognition")
        self.setMinimumSize(860, 640)
        self.resize(960, 720)
        cw = QWidget()
        self.setCentralWidget(cw)
        layout = QVBoxLayout(cw)
        layout.setContentsMargins(20, 16, 20, 16)
        layout.setSpacing(10)

        self.title_label = QLabel()
        self.title_label.setFont(system_font(22, bold=True))
        layout.addWidget(self.title_label)

        self.gpu_label = QLabel()
        layout.addWidget(self.gpu_label)

        top_bar = QHBoxLayout()
        top_bar.addStretch()
        self.theme_btn = QPushButton()
        self.theme_btn.setCheckable(True)
        self.theme_btn.setChecked(self.theme_dark)
        self.theme_btn.setFixedWidth(80)
        self.theme_btn.clicked.connect(self.toggle_theme)
        top_bar.addWidget(self.theme_btn)

        self.lang_combo = QComboBox()
        self.lang_combo.addItem("English", "en")
        self.lang_combo.addItem("中文", "zh")
        self.lang_combo.setCurrentIndex(0 if self.lang == "en" else 1)
        self.lang_combo.currentIndexChanged.connect(self.change_language)
        top_bar.addWidget(self.lang_combo)
        layout.addLayout(top_bar)

        device_row = QHBoxLayout()
        self.device_label = QLabel()
        device_row.addWidget(self.device_label)
        self.device_combo = QComboBox()
        self.device_combo.currentIndexChanged.connect(self.change_device)
        device_row.addWidget(self.device_combo)
        self._populate_device_combo()
        device_row.addStretch()
        layout.addLayout(device_row)

        self.cache_label = QLabel()
        self.cache_label.setStyleSheet("color: gray;")
        self.cache_label.setText(I18n.get("cache_dir_label", self.lang,
                                          path=get_whisper_cache_dir()))
        layout.addWidget(self.cache_label)

        language_row = QHBoxLayout()
        self.language_label = QLabel()
        language_row.addWidget(self.language_label)
        self.language_combo = QComboBox()
        self._populate_language_combo()
        language_row.addWidget(self.language_combo)
        language_row.addStretch()
        layout.addLayout(language_row)

        file_row = QHBoxLayout()
        self.file_label = QLabel()
        file_row.addWidget(self.file_label)
        self.file_edit = QLineEdit()
        self.file_edit.setReadOnly(True)
        file_row.addWidget(self.file_edit, 1)
        self.file_btn = QPushButton()
        self.file_btn.clicked.connect(self.select_file)
        file_row.addWidget(self.file_btn)
        layout.addLayout(file_row)

        out_row = QHBoxLayout()
        self.out_label = QLabel()
        out_row.addWidget(self.out_label)
        self.out_edit = QLineEdit(self.output_dir)
        out_row.addWidget(self.out_edit, 1)
        self.out_btn = QPushButton()
        self.out_btn.clicked.connect(self.select_output_dir)
        out_row.addWidget(self.out_btn)
        layout.addLayout(out_row)

        model_row = QHBoxLayout()
        self.model_label = QLabel()
        model_row.addWidget(self.model_label)
        self.model_combo = QComboBox()
        for m in WHISPER_MODELS:
            self.model_combo.addItem(I18n.get("model_" + m, self.lang), m)
        idx_m = self.model_combo.findData(self.recommended_model)
        if idx_m >= 0:
            self.model_combo.setCurrentIndex(idx_m)
        model_row.addWidget(self.model_combo, 1)
        self.rec_btn = QPushButton()
        self.rec_btn.clicked.connect(self.apply_recommendation)
        model_row.addWidget(self.rec_btn)
        layout.addLayout(model_row)

        self.rec_label = QLabel()
        self.rec_label.setStyleSheet("color: gray;")
        layout.addWidget(self.rec_label)

        self.splitter = QSplitter(Qt.Orientation.Vertical)
        self.result_view = QPlainTextEdit()
        self.result_view.setReadOnly(True)
        self.splitter.addWidget(self.result_view)

        self.log_view = QPlainTextEdit()
        self.log_view.setReadOnly(True)
        self.log_view.setMaximumBlockCount(2000)
        self.splitter.addWidget(self.log_view)
        self.splitter.setStretchFactor(0, 2)
        self.splitter.setStretchFactor(1, 1)
        layout.addWidget(self.splitter, 1)

        self.progress = QProgressBar()
        self.progress.setRange(0, 0)
        self.progress.setVisible(False)
        layout.addWidget(self.progress)

        btn_row = QHBoxLayout()
        self.install_cpu_btn = QPushButton()
        self.install_cpu_btn.clicked.connect(lambda: self._perform_install("cpu"))
        btn_row.addWidget(self.install_cpu_btn)

        self.install_gpu_btn = QPushButton()
        self.install_gpu_btn.clicked.connect(lambda: self._perform_install("gpu"))
        btn_row.addWidget(self.install_gpu_btn)

        self.install_model_btn = QPushButton()
        self.install_model_btn.clicked.connect(self._perform_model_install)
        btn_row.addWidget(self.install_model_btn)
        btn_row.addStretch()

        btn_font = DEFAULT_FONT
        self.install_cpu_btn.setFont(btn_font)
        self.install_gpu_btn.setFont(btn_font)
        self.install_model_btn.setFont(btn_font)

        self.start_btn = QPushButton()
        self.start_btn.setObjectName("primaryBtn")
        self.start_btn.setFont(btn_font)
        self.start_btn.clicked.connect(self.start_transcription)
        btn_row.addWidget(self.start_btn)
        self.stop_btn = QPushButton()
        self.stop_btn.setFont(btn_font)
        self.stop_btn.clicked.connect(self.stop_transcription)
        self.stop_btn.setEnabled(False)
        btn_row.addWidget(self.stop_btn)
        layout.addLayout(btn_row)

        self.install_cpu_btn.setText(I18n.get("install_btn_cpu", self.lang))
        self.install_gpu_btn.setText(I18n.get("install_btn_gpu", self.lang))
        self.install_model_btn.setText(I18n.get("install_btn_model", self.lang))

        self.status_bar = QStatusBar()
        self.setStatusBar(self.status_bar)

    def refresh_device_list(self):
        self._populate_device_combo()

    def _populate_device_combo(self):
        self.device_combo.blockSignals(True)
        self.device_combo.clear()
        for value in DEVICE_CHOICES:
            if value == "gpu" and not self.gpu_available:
                continue
            label_key = "auto" if value == "auto" else f"device_{value}"
            self.device_combo.addItem(I18n.get(label_key, self.lang), value)
        idx = self.device_combo.findData(self.user_device_choice)
        if idx >= 0:
            self.device_combo.setCurrentIndex(idx)
        self.device_combo.blockSignals(False)

    def _populate_language_combo(self):
        current = self.language_combo.currentData()
        self.language_combo.blockSignals(True)
        self.language_combo.clear()
        for code in SUPPORTED_LANGUAGES:
            self.language_combo.addItem(I18n.get("language_" + code, self.lang), code)
        idx = self.language_combo.findData(current)
        if idx >= 0:
            self.language_combo.setCurrentIndex(idx)
        self.language_combo.blockSignals(False)

    def _update_file_summary(self):
        if self.file_paths:
            self.file_edit.setText(I18n.get("file_summary", self.lang,
                                            count=len(self.file_paths),
                                            first=self.file_paths[0]))
        else:
            self.file_edit.clear()

    def change_device(self, idx):
        self.user_device_choice = self.device_combo.currentData()
        self.effective_device = self._resolve_effective_device()
        self.recommended_model = recommend_model(self.effective_device,
                                                 self.vram if self.gpu_available else 0)
        self.rec_label.setText(I18n.get("model_recommendation", self.lang,
                                         model=I18n.get("model_" + self.recommended_model, self.lang)))
        self._log_event(I18n.get("log_device_change", self.lang, device=self.user_device_choice.upper()))
        self.update_texts()

    def update_texts(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        self.title_label.setText(t("title"))
        gpu_info = t("gpu", name=self.gpu_name) if self.gpu_available else t("cpu")
        self.gpu_label.setText(gpu_info)
        self.device_label.setText(t("device_label"))
        self.language_label.setText(t("language_label"))
        self.file_label.setText(t("file_label"))
        self.file_btn.setText(t("browse"))
        self.out_label.setText(t("output_label"))
        self.out_btn.setText(t("output_browse"))
        self.model_label.setText(t("model_label"))
        self.rec_btn.setText(t("recommend"))
        self.rec_label.setText(t("model_recommendation", model=t("model_" + self.recommended_model)))
        self.start_btn.setText(t("start"))
        self.stop_btn.setText(t("stop"))
        self.install_cpu_btn.setText(t("install_btn_cpu"))
        self.install_gpu_btn.setText(t("install_btn_gpu"))
        self.install_model_btn.setText(t("install_btn_model"))
        self.cache_label.setText(t("cache_dir_label",
                                  path=get_whisper_cache_dir()))
        self.theme_btn.setText(t("dark" if self.theme_dark else "light"))
        self._refresh_model_list()
        self.refresh_device_list()
        self._populate_language_combo()
        self._update_file_summary()

    def _refresh_model_list(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        current = self.model_combo.currentData()
        cache_dir = get_whisper_cache_dir()
        self.model_combo.blockSignals(True)
        self.model_combo.clear()
        for m in WHISPER_MODELS:
            cache_file = os.path.join(cache_dir, f"{m}.pt")
            if os.path.isfile(cache_file):
                size_text = f"{os.path.getsize(cache_file) / (1024 * 1024):.0f}MB"
            else:
                size_text = f"{MODEL_REFERENCE_SIZES_MB[m]}MB"
            self.model_combo.addItem(f"{t('model_' + m)} ({size_text})", m)
        idx = self.model_combo.findData(current)
        if idx >= 0:
            self.model_combo.setCurrentIndex(idx)
        self.model_combo.blockSignals(False)

    def toggle_theme(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        self.theme_dark = not self.theme_dark
        self.apply_theme(self.theme_dark)
        self._log_event(t("log_theme_change", theme=t("dark" if self.theme_dark else "light")))

    def apply_theme(self, dark):
        icon_dir = ensure_arrow_icons().replace("\\", "/")
        arrow_name = "arrow_dark.svg" if dark else "arrow_light.svg"
        arrow_url = os.path.join(icon_dir, arrow_name).replace("\\", "/")
        palette = dark_palette() if dark else QApplication.style().standardPalette()
        QApplication.instance().setPalette(palette)
        stylesheet = dark_stylesheet(arrow_url) if dark else light_stylesheet(arrow_url)
        self.setStyleSheet(stylesheet)
        self.update_texts()

    def change_language(self, idx):
        self.lang = self.lang_combo.currentData()
        lang_name = "中文" if self.lang == "zh" else "English"
        self._log_event(I18n.get("log_language_changed", self.lang, name=lang_name))
        self.update_texts()

    def select_file(self):
        t = lambda key: I18n.get(key, self.lang)
        paths, _ = QFileDialog.getOpenFileNames(self, t("browse_file"), "", t("audio_filter"))
        if paths:
            self.file_paths = paths
            self._update_file_summary()
            auto_out = os.path.dirname(paths[0])
            self.out_edit.setText(auto_out)
            self.output_dir = auto_out
            self._log_event(I18n.get("log_files_selected", self.lang, count=len(paths)))

    def select_output_dir(self):
        t = lambda key: I18n.get(key, self.lang)
        path = QFileDialog.getExistingDirectory(self, t("output_label"), self.out_edit.text())
        if path:
            self.out_edit.setText(path)
            self.output_dir = path
            self._log_event(I18n.get("log_output_dir_changed", self.lang, dir=path))

    def apply_recommendation(self):
        idx = self.model_combo.findData(self.recommended_model)
        if idx >= 0:
            self.model_combo.setCurrentIndex(idx)
            self._log_event(I18n.get("log_model_recommended", self.lang,
                                     model=I18n.get("model_" + self.recommended_model, self.lang)))

    def _current_torch_matches(self, target_device):
        try:
            import torch
            has_cuda = torch.cuda.is_available()
        except ImportError:
            return False
        if target_device == "gpu":
            return has_cuda
        return not has_cuda

    def _describe_current_torch(self):
        try:
            import torch
            if torch.cuda.is_available():
                return I18n.get("current_torch_gpu", self.lang)
            return I18n.get("current_torch_cpu", self.lang)
        except ImportError:
            return I18n.get("current_torch_none", self.lang)

    def _perform_model_install(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        model_names = WHISPER_MODELS
        display_names = [f"{t('model_' + m)} ({MODEL_REFERENCE_SIZES_MB[m]}MB)" for m in model_names]
        display, ok = QInputDialog.getItem(
            self, t("model_install_title"), t("model_install_prompt"),
            display_names, 0, False
        )
        if not ok or not display:
            return
        model = model_names[display_names.index(display)]

        model_file = os.path.join(get_whisper_cache_dir(), f"{model}.pt")
        if os.path.isfile(model_file):
            size_mb = os.path.getsize(model_file) / (1024 * 1024)
            QMessageBox.information(self, t("model_install_title"),
                                    t("model_already_installed", model=model))
            self._append_log(t("model_already_installed", model=model) + f" ({size_mb:.1f} MB)")
            return

        self.install_model_btn.setEnabled(False)
        self.progress.setRange(0, 100)
        self.progress.setValue(0)
        self.progress.setVisible(True)
        self.model_download_thread = ModelDownloadThread(model, self.lang)
        self.model_download_thread.log.connect(self._append_log)
        self.model_download_thread.progress.connect(self._on_model_download_progress)
        self.model_download_thread.done.connect(self._on_model_download_done)
        self.model_download_thread.start()

    def _on_model_download_progress(self, percent, total):
        self.progress.setValue(percent)

    def _on_model_download_done(self, success, message):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        self._append_log(message)
        self.progress.setVisible(False)
        if success:
            self.status_bar.showMessage(message)
            QMessageBox.information(self, t("model_install_title"), message)
        else:
            QMessageBox.critical(self, t("error_title"), message)
        self.install_model_btn.setEnabled(True)
        self.status_bar.showMessage(I18n.get("ready", self.lang))

    def _perform_install(self, target_device):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)

        if target_device == "gpu" and platform.system() == "Darwin":
            QMessageBox.warning(self, t("error_title"), t("gpu_not_supported_macos"))
            return

        if self._current_torch_matches(target_device):
            key = "already_installed_gpu" if target_device == "gpu" else "already_installed_cpu"
            QMessageBox.information(self, t("install_confirm_title"), t(key))
            return
        if getattr(sys, "frozen", False) and not get_python_command():
            QMessageBox.warning(self, t("error_title"), t("packaged_install_requires_python"))
            return

        current_desc = self._describe_current_torch()
        confirm_key = "install_confirm_gpu" if target_device == "gpu" else "install_confirm_cpu"
        msg = current_desc + "\n\n" + t(confirm_key)
        reply = QMessageBox.question(
            self, t("install_confirm_title"),
            msg,
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        )
        if reply != QMessageBox.StandardButton.Yes:
            return

        self.install_cpu_btn.setEnabled(False)
        self.install_gpu_btn.setEnabled(False)
        self.status_bar.showMessage(t("installing"))
        self._append_log(t("checking_deps"))

        self.install_thread = InstallThread(target_device, self.lang)
        self.install_thread.log.connect(self._append_log)
        self.install_thread.install_done.connect(self._on_install_done)
        self.install_thread.start()

    def _restart_app(self):
        try:
            entry = os.path.abspath(sys.argv[0]) if os.path.isfile(sys.argv[0]) else os.path.abspath(__file__)
            subprocess.Popen([sys.executable, entry])
        except Exception:
            pass
        QApplication.instance().quit()

    def _on_install_done(self, success, changed, message):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        if success:
            MODEL_CACHE.clear()
            try:
                has_cuda, self.gpu_name, self.vram = detect_gpu()
                self.gpu_available = has_cuda
                self.effective_device = self._resolve_effective_device()
                self.recommended_model = recommend_model(self.effective_device,
                                                         self.vram if self.gpu_available else 0)
                self.apply_recommendation()
                self.update_texts()
            except Exception as e:
                self._append_log(t("log_import_error", e=e))
            self._append_log(message)
            self.status_bar.showMessage(message)
            if changed:
                reply = QMessageBox.question(
                    self, t("restart_confirm_title"),
                    t("restart_confirm_msg"),
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
                )
                if reply == QMessageBox.StandardButton.Yes:
                    self._restart_app()
            else:
                QMessageBox.information(self, t("install_confirm_title"), t("install_already_done"))
        else:
            self._append_log(message)
            QMessageBox.critical(self, t("error_title"), message)

        self.install_cpu_btn.setEnabled(True)
        self.install_gpu_btn.setEnabled(True)
        self.status_bar.showMessage(I18n.get("ready", self.lang))

    def start_transcription(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        missing = [path for path in self.file_paths if not os.path.isfile(path)]
        if not self.file_paths or missing:
            QMessageBox.warning(self, t("error_title"), t("select_file"))
            return
        out_dir = self.out_edit.text()
        if not out_dir or not os.path.isdir(out_dir):
            QMessageBox.warning(self, t("error_title"), t("invalid_dir"))
            return
        self.output_dir = out_dir

        device = self.get_effective_device()
        model_name = self.model_combo.currentData()
        transcribe_language = self.language_combo.currentData()
        self.last_error = None
        self.manual_stop = False
        self._log_event(I18n.get("log_start_transcription", self.lang, model=model_name, device=device))

        self.start_btn.setEnabled(False)
        self.stop_btn.setEnabled(True)
        self.progress.setRange(0, len(self.file_paths))
        self.progress.setValue(0)
        self.progress.setFormat("%v / %m")
        self.progress.setVisible(True)
        self.status_bar.showMessage(f"{I18n.get('transcribing', self.lang)} ({self._device_display_name(device)})")
        self.log_view.clear()
        self.result_view.clear()

        self.thread = TranscriptionThread(
            self.file_paths,
            model_name,
            device,
            self.output_dir,
            ui_lang=self.lang,
            transcribe_language=transcribe_language,
        )
        self.thread.status.connect(self.status_bar.showMessage)
        self.thread.log.connect(self._append_log)
        self.thread.progress.connect(self._on_transcription_progress)
        self.thread.result.connect(self._on_transcription_result)
        self.thread.error.connect(self._on_transcription_error)
        self.thread.finished.connect(self.on_finished)
        self.thread.start()

    def _on_transcription_progress(self, current, total):
        self.progress.setRange(0, total)
        self.progress.setValue(current)

    def _on_transcription_result(self, text):
        existing = self.result_view.toPlainText()
        self.result_view.setPlainText(existing + text if existing else text)

    def _on_transcription_error(self, message):
        self.last_error = message
        self._append_log(I18n.get("log_error_prefix", self.lang, e=message))

    def stop_transcription(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        if self.thread and self.thread.isRunning():
            reply = QMessageBox.question(
                self, t("force_stop_title"),
                t("force_stop_confirm"),
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )
            if reply == QMessageBox.StandardButton.Yes:
                self.manual_stop = True
                try:
                    self.thread.finished.disconnect(self.on_finished)
                except TypeError:
                    pass
                self.thread.terminate()
                self.thread.wait(2000)
                self._log_event(I18n.get("force_stop_executed", self.lang))
                self.status_bar.showMessage(I18n.get("force_stop_executed", self.lang))
                self.on_finished()
        else:
            self.status_bar.showMessage(I18n.get("stop_note", self.lang))

    def on_finished(self):
        self.start_btn.setEnabled(True)
        self.stop_btn.setEnabled(False)
        self.progress.setVisible(False)
        if self.last_error:
            self.status_bar.showMessage(I18n.get("error", self.lang, msg=self.last_error))
        elif self.manual_stop:
            self.status_bar.showMessage(I18n.get("force_stop_executed", self.lang))
        else:
            saved_count = len(self.thread.output_paths) if self.thread else 0
            self.status_bar.showMessage(I18n.get("batch_done", self.lang, count=saved_count))
            self._append_log(I18n.get("log_transcription_finished", self.lang))

    def closeEvent(self, event):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        active_threads = [
            thread for thread in (
                self.thread,
                getattr(self, "model_download_thread", None),
                getattr(self, "install_thread", None),
            )
            if thread is not None and thread.isRunning()
        ]
        if active_threads:
            reply = QMessageBox.question(
                self, t("close_task_title"), t("close_task_msg"),
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )
            if reply != QMessageBox.StandardButton.Yes:
                event.ignore()
                return
            for thread in active_threads:
                thread.terminate()
                thread.wait(2000)
        self._log_event(I18n.get("log_app_closing", self.lang))
        self.settings.setValue("lang", self.lang)
        self.settings.setValue("dark", self.theme_dark)
        self.settings.setValue("device", self.user_device_choice)
        self.settings.setValue("geometry", self.saveGeometry())
        self.settings.setValue("splitter", self.splitter.saveState())
        self.settings.setValue("output_dir", self.output_dir)
        super().closeEvent(event)

    def restore_state(self):
        geom = self.settings.value("geometry")
        if geom:
            self.restoreGeometry(geom)
        split = self.settings.value("splitter")
        if split:
            self.splitter.restoreState(split)
        saved_out = self.settings.value("output_dir")
        if saved_out and os.path.isdir(saved_out):
            self.output_dir = saved_out
            self.out_edit.setText(saved_out)


def main():
    app = QApplication(sys.argv)
    app.setStyle(QStyleFactory.create("Fusion"))
    window = MainWindow()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
