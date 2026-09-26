import datetime
import os
import platform
import subprocess
import sys

from .bootstrap import ensure_gui_dependency, find_missing_modules, get_python_command

ensure_gui_dependency()

from PyQt6.QtCore import Qt, QSettings
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QLineEdit, QPushButton, QProgressBar, QPlainTextEdit,
    QComboBox, QFileDialog, QSplitter, QStatusBar, QStyleFactory, QMessageBox,
    QInputDialog,
)

from .constants import DEVICE_CHOICES, MODEL_REFERENCE_SIZES_MB, SUPPORTED_LANGUAGES, WHISPER_MODELS
from .download import (
    clear_model_cache,
    get_cached_model_size_mb,
    get_whisper_cache_dir,
    is_model_cached,
)
from .i18n import I18n
from .threads import InstallThread, ModelDownloadThread, TranscriptionThread
from .utils import (
    DARKDETECT,
    DEFAULT_FONT,
    dark_palette,
    dark_stylesheet,
    darkdetect_is_dark,
    detect_gpu,
    ensure_arrow_icons,
    get_system_language,
    has_ffmpeg,
    is_admin,
    light_stylesheet,
    recommend_model,
    system_font,
)

YES = QMessageBox.StandardButton.Yes
NO = QMessageBox.StandardButton.No


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.settings = QSettings("WhisperGUI", "UserPrefs")
        self.lang = self.settings.value("lang", get_system_language())
        if self.lang not in I18n.translations:
            self.lang = "en"
        dark_default = DARKDETECT and darkdetect_is_dark()
        self.theme_dark = self.settings.value("dark", dark_default, bool)

        has_cuda, gpu_name, vram = detect_gpu()
        self.gpu_available = has_cuda
        self.gpu_name = gpu_name
        self.vram = vram
        self.has_ffmpeg = has_ffmpeg()
        self.is_macos = platform.system() == "Darwin"

        self.user_device_choice = self.settings.value("device", "auto")
        if self.user_device_choice not in DEVICE_CHOICES:
            self.user_device_choice = "auto"

        self.effective_device = self._resolve_effective_device()
        self.recommended_model = recommend_model(self.effective_device, self._usable_vram())
        self.output_dir = self.settings.value("output_dir", os.path.expanduser("~"))
        self.transcription_thread = None
        self.model_download_thread = None
        self.install_thread = None
        self.file_paths = []
        self.last_error = None
        self.last_failure_count = 0
        self.manual_stop = False

        self.init_ui()
        self.apply_theme(self.theme_dark)
        self.update_texts()
        self.restore_state()

        self._log_event(self._t("log_app_started"))
        if not is_admin():
            self._append_log("[!] %s" % self._t("admin_warning"))
        self._log_hardware()
        self._log_event(self._t("ready"))
        self.status_bar.showMessage(self._t("ready"))

    def _t(self, key, **kwargs):
        return I18n.get(key, self.lang, **kwargs)

    def _entry(self, key, **kwargs):
        return self._t(key).format(**kwargs) if kwargs else self._t(key)

    def _log_hardware(self):
        if self.gpu_available:
            self._log_event(self._entry("gpu_detected", name=self.gpu_name, mem=self.vram))
        else:
            self._log_event(self._t("no_gpu"))
        if not self.has_ffmpeg:
            self._append_log("[!] %s" % self._t("ffmpeg_missing"))

    def _usable_vram(self):
        return self.vram if self.gpu_available else 0

    def _append_log(self, msg):
        self.log_view.appendPlainText(msg)
        scrollbar = self.log_view.verticalScrollBar()
        scrollbar.setValue(scrollbar.maximum())

    def _log_event(self, msg):
        stamp = datetime.datetime.now().strftime("%H:%M:%S")
        self._append_log("[%s] %s" % (stamp, msg))

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
            return self._t("device_gpu")
        if device == "mps":
            return self._t("device_mps")
        return self._t("device_cpu")

    def _model_display_name(self, model):
        return self._t("model_%s" % model)

    def init_ui(self):
        self.setWindowTitle("Whisper Speech Recognition")
        self.setMinimumSize(860, 640)
        self.resize(960, 720)
        central = QWidget()
        self.setCentralWidget(central)
        layout = QVBoxLayout(central)
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
        layout.addWidget(self.cache_label)

        self.ffmpeg_label = QLabel()
        self.ffmpeg_label.setStyleSheet("color: #b36b00;")
        layout.addWidget(self.ffmpeg_label)

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
        self._refresh_model_list()
        model_row.addWidget(self.model_combo, 1)
        self.rec_btn = QPushButton()
        self.rec_btn.clicked.connect(lambda: self.apply_recommendation())
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

        install_buttons = (self.install_cpu_btn, self.install_gpu_btn, self.install_model_btn)
        for button in install_buttons:
            button.setFont(DEFAULT_FONT)

        self.start_btn = QPushButton()
        self.start_btn.setObjectName("primaryBtn")
        self.start_btn.setFont(DEFAULT_FONT)
        self.start_btn.clicked.connect(self.start_transcription)
        btn_row.addWidget(self.start_btn)
        self.stop_btn = QPushButton()
        self.stop_btn.setFont(DEFAULT_FONT)
        self.stop_btn.clicked.connect(self.stop_transcription)
        self.stop_btn.setEnabled(False)
        btn_row.addWidget(self.stop_btn)
        layout.addLayout(btn_row)

        self.status_bar = QStatusBar()
        self.setStatusBar(self.status_bar)
        self._update_buttons()

    def _active_threads(self):
        candidates = (self.transcription_thread, self.model_download_thread, self.install_thread)
        return [t for t in candidates if t is not None and t.isRunning()]

    def _update_buttons(self):
        transcribing = self.transcription_thread is not None and self.transcription_thread.isRunning()
        background = self._background_task_running()
        locked = transcribing or background
        self.install_cpu_btn.setEnabled(not locked)
        self.install_gpu_btn.setEnabled(not locked and not self.is_macos)
        self.install_model_btn.setEnabled(not locked)
        self.start_btn.setEnabled(not locked)
        self.stop_btn.setEnabled(transcribing)
        if self.is_macos:
            self.install_gpu_btn.setText(self._t("install_btn_unavailable"))
            self.install_gpu_btn.setToolTip(self._t("gpu_not_supported_macos"))
        else:
            self.install_gpu_btn.setText(self._t("install_btn_gpu"))
            self.install_gpu_btn.setToolTip("")

    def _background_task_running(self):
        running = (self.install_thread, self.model_download_thread)
        return any(t is not None and t.isRunning() for t in running)

    def _populate_device_combo(self):
        self.device_combo.blockSignals(True)
        self.device_combo.clear()
        for value in DEVICE_CHOICES:
            if value == "gpu" and not self.gpu_available:
                continue
            label_key = "auto" if value == "auto" else "device_%s" % value
            self.device_combo.addItem(self._t(label_key), value)
        index = self.device_combo.findData(self.user_device_choice)
        if index < 0:
            index = self.device_combo.findData("auto")
        self.device_combo.setCurrentIndex(max(0, index))
        self.user_device_choice = self.device_combo.currentData()
        self.device_combo.blockSignals(False)

    def _populate_language_combo(self):
        current = self.language_combo.currentData()
        self.language_combo.blockSignals(True)
        self.language_combo.clear()
        for code in SUPPORTED_LANGUAGES:
            self.language_combo.addItem(self._t("language_%s" % code), code)
        index = self.language_combo.findData(current)
        self.language_combo.setCurrentIndex(index if index >= 0 else 0)
        self.language_combo.blockSignals(False)

    def _refresh_model_list(self):
        current = self.model_combo.currentData()
        self.model_combo.blockSignals(True)
        self.model_combo.clear()
        for model in WHISPER_MODELS:
            size_mb = get_cached_model_size_mb(model) or MODEL_REFERENCE_SIZES_MB[model]
            label = "%s (%dMB)" % (self._model_display_name(model), round(size_mb))
            self.model_combo.addItem(label, model)
        index = self.model_combo.findData(current)
        if index < 0:
            index = self.model_combo.findData(self.recommended_model)
        self.model_combo.setCurrentIndex(index if index >= 0 else 0)
        self.model_combo.blockSignals(False)

    def _update_file_summary(self):
        if not self.file_paths:
            self.file_edit.clear()
            return
        entry = self._t("file_summary")
        self.file_edit.setText(entry.format(count=len(self.file_paths), first=self.file_paths[0]))

    def change_device(self, _index):
        self.user_device_choice = self.device_combo.currentData()
        self.effective_device = self._resolve_effective_device()
        self.recommended_model = recommend_model(self.effective_device, self._usable_vram())
        self.apply_recommendation(log_event=False)
        label = self.user_device_choice.upper()
        self._log_event(self._entry("log_device_change", device=label))
        self.update_texts()

    def change_language(self, _index):
        self.lang = self.lang_combo.currentData()
        name = self.lang_combo.currentText()
        self._log_event(self._entry("log_language_changed", name=name))
        self.update_texts()

    def toggle_theme(self):
        self.theme_dark = not self.theme_dark
        self.apply_theme(self.theme_dark)
        label = self._t("dark") if self.theme_dark else self._t("light")
        self._log_event(self._entry("log_theme_change", theme=label))

    def apply_theme(self, dark):
        icon_dir = ensure_arrow_icons().replace("\\", "/")
        arrow_name = "arrow_dark.svg" if dark else "arrow_light.svg"
        arrow_url = "%s/%s" % (icon_dir, arrow_name)
        application = QApplication.instance()
        application.setPalette(dark_palette() if dark else QApplication.style().standardPalette())
        self.setStyleSheet(dark_stylesheet(arrow_url) if dark else light_stylesheet(arrow_url))
        self.update_texts()

    def update_texts(self):
        self.title_label.setText(self._t("title"))
        if self.gpu_available:
            self.gpu_label.setText(self._entry("gpu", name=self.gpu_name))
        else:
            self.gpu_label.setText(self._t("cpu"))
        self.device_label.setText(self._t("device_label"))
        self.language_label.setText(self._t("language_label"))
        self.file_label.setText(self._t("file_label"))
        self.file_btn.setText(self._t("browse"))
        self.out_label.setText(self._t("output_label"))
        self.out_btn.setText(self._t("output_browse"))
        self.model_label.setText(self._t("model_label"))
        self.rec_btn.setText(self._t("recommend"))
        recommended = self._model_display_name(self.recommended_model)
        self.rec_label.setText(self._entry("model_recommendation", model=recommended))
        self.start_btn.setText(self._t("start"))
        self.stop_btn.setText(self._t("stop"))
        self.install_cpu_btn.setText(self._t("install_btn_cpu"))
        self.install_model_btn.setText(self._t("install_btn_model"))
        cache_path = get_whisper_cache_dir()
        self.cache_label.setText(self._entry("cache_dir_label", path=cache_path))
        self.ffmpeg_label.setText("" if self.has_ffmpeg else self._t("ffmpeg_missing_label"))
        self.ffmpeg_label.setVisible(not self.has_ffmpeg)
        theme_label = self._t("dark") if self.theme_dark else self._t("light")
        self.theme_btn.setText(theme_label)
        self._refresh_model_list()
        self._populate_device_combo()
        self._populate_language_combo()
        self._update_file_summary()
        self._update_buttons()

    def select_file(self):
        paths, _ = QFileDialog.getOpenFileNames(
            self, self._t("browse_file"), "", self._t("audio_filter"),
        )
        if not paths:
            return
        self.file_paths = paths
        self._update_file_summary()
        self._log_event(self._entry("log_files_selected", count=len(paths)))

    def select_output_dir(self):
        current = self.out_edit.text()
        path = QFileDialog.getExistingDirectory(self, self._t("output_label"), current)
        if not path:
            return
        self.out_edit.setText(path)
        self.output_dir = path
        self._log_event(self._entry("log_output_dir_changed", dir=path))

    def apply_recommendation(self, log_event=True):
        index = self.model_combo.findData(self.recommended_model)
        if index < 0:
            return
        self.model_combo.setCurrentIndex(index)
        if log_event:
            name = self._model_display_name(self.recommended_model)
            self._log_event(self._entry("log_model_recommended", model=name))

    def _current_torch_matches(self, target_device):
        try:
            import torch
            has_cuda = torch.cuda.is_available()
        except ImportError:
            return False
        return has_cuda if target_device == "gpu" else not has_cuda

    def _describe_current_torch(self):
        try:
            import torch
            if torch.cuda.is_available():
                return self._t("current_torch_gpu")
            return self._t("current_torch_cpu")
        except ImportError:
            return self._t("current_torch_none")

    def _model_label_with_size(self, model):
        size_mb = get_cached_model_size_mb(model) or MODEL_REFERENCE_SIZES_MB[model]
        return "%s (%dMB)" % (self._model_display_name(model), round(size_mb))

    def _perform_model_install(self):
        names = list(WHISPER_MODELS)
        labels = [self._model_label_with_size(model) for model in names]
        choice, accepted = QInputDialog.getItem(
            self, self._t("model_install_title"), self._t("model_install_prompt"),
            labels, 0, False,
        )
        if not accepted or choice not in labels:
            return
        model = names[labels.index(choice)]
        if is_model_cached(model):
            size_mb = get_cached_model_size_mb(model)
            message = self._entry("model_already_installed", model=model)
            QMessageBox.information(self, self._t("model_install_title"), message)
            self._append_log("%s (%.1f MB)" % (message, size_mb))
            return
        self._download_model(model)

    def _download_model(self, model):
        self.progress.setRange(0, 100)
        self.progress.setValue(0)
        self.progress.setVisible(True)
        self.model_download_thread = ModelDownloadThread(model, self.lang)
        self.model_download_thread.log.connect(self._append_log)
        self.model_download_thread.progress.connect(self._on_model_download_progress)
        self.model_download_thread.done.connect(self._on_model_download_done)
        self._update_buttons()
        self.model_download_thread.start()

    def _on_model_download_progress(self, percent, _total):
        self.progress.setValue(percent)

    def _on_model_download_done(self, success, message):
        self._append_log(message)
        self.progress.setVisible(False)
        self.progress.setRange(0, 0)
        if success:
            self.status_bar.showMessage(message)
            QMessageBox.information(self, self._t("model_install_title"), message)
        else:
            QMessageBox.critical(self, self._t("error_title"), message)
        self._refresh_model_list()
        self._update_buttons()
        self.status_bar.showMessage(self._t("ready"))

    def _perform_install(self, target_device):
        if self._active_threads():
            return
        if target_device == "gpu" and self.is_macos:
            QMessageBox.warning(self, self._t("error_title"), self._t("gpu_not_supported_macos"))
            return
        if self._current_torch_matches(target_device):
            key = "already_installed_gpu" if target_device == "gpu" else "already_installed_cpu"
            QMessageBox.information(self, self._t("install_confirm_title"), self._t(key))
            return
        if getattr(sys, "frozen", False) and not get_python_command():
            message = self._t("packaged_install_requires_python")
            QMessageBox.warning(self, self._t("error_title"), message)
            return
        confirm_key = "install_confirm_gpu" if target_device == "gpu" else "install_confirm_cpu"
        body = "%s\n\n%s" % (self._describe_current_torch(), self._t(confirm_key))
        reply = QMessageBox.question(
            self, self._t("install_confirm_title"), body, YES | NO, NO,
        )
        if reply != YES:
            return
        self.status_bar.showMessage(self._t("installing"))
        self._append_log(self._t("checking_deps"))
        self.install_thread = InstallThread(target_device, self.lang)
        self.install_thread.log.connect(self._append_log)
        self.install_thread.install_done.connect(self._on_install_done)
        self._update_buttons()
        self.install_thread.start()

    def _restart_app(self):
        try:
            if getattr(sys, "frozen", False):
                subprocess.Popen([sys.executable])
            else:
                root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
                entry = os.path.join(root, "WhisperGUI.py")
                subprocess.Popen([sys.executable, entry], cwd=root)
        except OSError:
            pass
        QApplication.instance().quit()

    def _on_install_done(self, success, changed, message):
        if not success:
            self._append_log(message)
            QMessageBox.critical(self, self._t("error_title"), message)
            self._update_buttons()
            self.status_bar.showMessage(self._t("ready"))
            return
        clear_model_cache()
        self._refresh_hardware_state()
        self._append_log(message)
        self.status_bar.showMessage(message)
        if changed:
            reply = QMessageBox.question(
                self, self._t("restart_confirm_title"),
                self._t("restart_confirm_msg"), YES | NO, NO,
            )
            if reply == YES:
                self._restart_app()
                return
        else:
            QMessageBox.information(self, self._t("install_confirm_title"),
                                    self._t("install_already_done"))
        self._update_buttons()
        self.status_bar.showMessage(self._t("ready"))

    def _refresh_hardware_state(self):
        try:
            has_cuda, gpu_name, vram = detect_gpu()
        except (AssertionError, RuntimeError):
            return
        self.gpu_available = has_cuda
        self.gpu_name = gpu_name
        self.vram = vram
        self.effective_device = self._resolve_effective_device()
        self.recommended_model = recommend_model(self.effective_device, self._usable_vram())
        self.apply_recommendation(log_event=False)
        self.update_texts()

    def _all_files_are_wav(self):
        return all(path.lower().endswith(".wav") for path in self.file_paths)

    def _preflight(self):
        missing = find_missing_modules()
        if missing:
            names = ", ".join(missing)
            QMessageBox.warning(self, self._t("error_title"),
                                self._entry("deps_missing", list=names))
            return False
        model = self.model_combo.currentData()
        if not is_model_cached(model):
            body = "%s\n\n%s" % (self._entry("model_not_downloaded", model=model),
                                 self._t("download_first"))
            reply = QMessageBox.question(
                self, self._t("warning_title"), body, YES | NO, YES,
            )
            if reply != YES:
                return False
            self._download_model(model)
            return False
        if not self.has_ffmpeg and not self._all_files_are_wav():
            QMessageBox.warning(self, self._t("error_title"), self._t("ffmpeg_missing"))
            return False
        return True

    def start_transcription(self):
        if not self.file_paths:
            QMessageBox.warning(self, self._t("error_title"), self._t("select_file"))
            return
        missing = [path for path in self.file_paths if not os.path.isfile(path)]
        if missing:
            joined = "\n".join(missing)
            QMessageBox.warning(self, self._t("error_title"),
                                self._entry("missing_files", paths=joined))
            return
        out_dir = self.out_edit.text().strip()
        if not out_dir or not os.path.isdir(out_dir):
            QMessageBox.warning(self, self._t("error_title"), self._t("invalid_dir"))
            return
        if not self._preflight():
            return
        self._begin_transcription(out_dir)

    def _begin_transcription(self, out_dir):
        self.output_dir = out_dir
        device = self.get_effective_device()
        model_name = self.model_combo.currentData()
        self.last_error = None
        self.last_failure_count = 0
        self.manual_stop = False
        entry = self._entry("log_start_transcription", model=model_name, device=device)
        self._log_event(entry)
        self.start_btn.setEnabled(False)
        self.stop_btn.setEnabled(True)
        self.progress.setRange(0, len(self.file_paths))
        self.progress.setValue(0)
        self.progress.setFormat("%v / %m")
        self.progress.setVisible(True)
        label = self._device_display_name(device)
        self.status_bar.showMessage("%s (%s)" % (self._t("transcribing"), label))
        self.log_view.clear()
        self.result_view.clear()
        self.transcription_thread = TranscriptionThread(
            self.file_paths,
            model_name,
            device,
            self.output_dir,
            ui_lang=self.lang,
            transcribe_language=self.language_combo.currentData(),
        )
        thread = self.transcription_thread
        thread.status.connect(self.status_bar.showMessage)
        thread.log.connect(self._append_log)
        thread.progress.connect(self._on_transcription_progress)
        thread.result.connect(self._on_transcription_result)
        thread.file_failed.connect(self._on_file_failed)
        thread.error.connect(self._on_transcription_error)
        thread.finished.connect(self.on_finished)
        thread.start()

    def _on_transcription_progress(self, current, total):
        self.progress.setRange(0, total)
        self.progress.setValue(current)

    def _on_transcription_result(self, text):
        existing = self.result_view.toPlainText()
        self.result_view.setPlainText(existing + text if existing else text)

    def _on_transcription_error(self, message):
        self.last_error = message
        self._append_log(self._entry("log_error_prefix", e=message))

    def _on_file_failed(self, message):
        self._append_log(self._entry("log_error_prefix", e=message))

    def stop_transcription(self):
        thread = self.transcription_thread
        if thread is None or not thread.isRunning():
            self.status_bar.showMessage(self._t("stop_note"))
            return
        reply = QMessageBox.question(
            self, self._t("force_stop_title"), self._t("force_stop_confirm"), YES | NO, NO,
        )
        if reply != YES:
            return
        self.manual_stop = True
        thread.request_cancel()
        try:
            thread.finished.disconnect(self.on_finished)
        except TypeError:
            pass
        if not thread.wait(3000):
            thread.terminate()
            thread.wait(2000)
        thread.restore_streams()
        self._log_event(self._t("force_stop_executed"))
        self.status_bar.showMessage(self._t("force_stop_executed"))
        self.on_finished()

    def on_finished(self):
        self.progress.setVisible(False)
        self.progress.setFormat("")
        self.progress.setRange(0, 0)
        self._update_buttons()
        if self.manual_stop:
            self.status_bar.showMessage(self._t("force_stop_executed"))
            return
        if self.last_error:
            self.status_bar.showMessage(self._entry("error", msg=self.last_error))
            return
        thread = self.transcription_thread
        saved = len(thread.output_paths) if thread else 0
        failed = len(thread.failed_files) if thread else 0
        if failed:
            entry = self._t("batch_partial")
            self.status_bar.showMessage(entry.format(count=saved, failed=failed))
            return
        self.status_bar.showMessage(self._entry("batch_done", count=saved))
        self._append_log(self._t("log_transcription_finished"))

    def _save_settings(self):
        self.settings.setValue("lang", self.lang)
        self.settings.setValue("dark", self.theme_dark)
        self.settings.setValue("device", self.user_device_choice)
        self.settings.setValue("geometry", self.saveGeometry())
        self.settings.setValue("splitter", self.splitter.saveState())
        self.settings.setValue("output_dir", self.out_edit.text().strip())
        self.settings.sync()

    def closeEvent(self, event):
        active = self._active_threads()
        if active:
            reply = QMessageBox.question(
                self, self._t("close_task_title"), self._t("close_task_msg"), YES | NO, NO,
            )
            if reply != YES:
                event.ignore()
                return
            self._shutdown_threads(active)
        self._log_event(self._t("log_app_closing"))
        self._save_settings()
        super().closeEvent(event)

    def _shutdown_threads(self, active):
        for thread in active:
            if hasattr(thread, "request_cancel"):
                thread.request_cancel()
            if not thread.wait(2000):
                thread.terminate()
                thread.wait(2000)
        if self.transcription_thread is not None:
            self.transcription_thread.restore_streams()

    def restore_state(self):
        geometry = self.settings.value("geometry")
        if geometry:
            self.restoreGeometry(geometry)
        splitter = self.settings.value("splitter")
        if splitter:
            self.splitter.restoreState(splitter)
        saved_out = self.settings.value("output_dir")
        if saved_out and os.path.isdir(saved_out):
            self.output_dir = saved_out
            self.out_edit.setText(saved_out)


def main():
    application = QApplication(sys.argv)
    application.setStyle(QStyleFactory.create("Fusion"))
    window = MainWindow()
    window.show()
    sys.exit(application.exec())


if __name__ == "__main__":
    main()
