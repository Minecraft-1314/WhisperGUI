import os
import sys
import importlib

from PyQt6.QtCore import QThread, pyqtSignal

from .bootstrap import check_transcription_deps, install_dependency, uninstall_package
from .constants import TORCH_INDEX_URLS, TORCH_PACKAGES
from .download import (
    download_file_single,
    get_model_download_url,
    get_remote_size,
    get_whisper_cache_dir,
    load_whisper_model,
)
from .i18n import I18n
from .utils import LogStream


class TranscriptionThread(QThread):
    status = pyqtSignal(str)
    log = pyqtSignal(str)
    result = pyqtSignal(str)
    error = pyqtSignal(str)
    progress = pyqtSignal(int, int)

    def __init__(self, file_paths, model_name, device, output_dir, ui_lang="en",
                 transcribe_language=None, parent=None):
        super().__init__(parent)
        self.file_paths = list(file_paths)
        self.model_name = model_name
        self.device = device
        self.output_dir = output_dir
        self.ui_lang = ui_lang
        self.transcribe_language = None if transcribe_language in (None, "auto") else transcribe_language
        self.output_paths = []

    def run(self):
        original_stdout = sys.stdout
        try:
            sys.stdout = LogStream(self.log)
            self.status.emit(I18n.get("transcribing", self.ui_lang))
            model = load_whisper_model(self.model_name, self.device)
            total = len(self.file_paths)
            for index, file_path in enumerate(self.file_paths, start=1):
                self.progress.emit(index - 1, total)
                self.log.emit(I18n.get("log_batch_file", self.ui_lang,
                                       current=index, total=total, path=file_path))
                result = model.transcribe(
                    file_path,
                    language=self.transcribe_language,
                    fp16=(self.device == "cuda"),
                    verbose=True
                )
                text = result["text"].strip()
                if self.transcribe_language is None:
                    detected = result.get("language")
                    if detected:
                        self.log.emit(I18n.get("log_language_detected", self.ui_lang,
                                               language=detected))
                base = os.path.splitext(os.path.basename(file_path))[0]
                out_path = os.path.join(self.output_dir, f"{base}_transcript.txt")
                os.makedirs(self.output_dir, exist_ok=True)
                with open(out_path, "w", encoding="utf-8") as f:
                    f.write(text)
                self.output_paths.append(out_path)
                self.result.emit(f"[{index}/{total}] {os.path.basename(file_path)}\n{text}\n")
                self.log.emit(I18n.get("log_saved_to", self.ui_lang, path=out_path))
            self.progress.emit(total, total)
        except Exception as e:
            self.error.emit(str(e))
        finally:
            sys.stdout = original_stdout


class InstallThread(QThread):
    log = pyqtSignal(str)
    install_done = pyqtSignal(bool, bool, str)

    def __init__(self, target_device, lang, parent=None):
        super().__init__(parent)
        self.target_device = target_device
        self.lang = lang

    def run(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        current_has_cuda = False
        torch_installed = False
        try:
            import torch
            current_has_cuda = torch.cuda.is_available()
            torch_installed = True
        except ImportError:
            pass

        need_install_torch = True
        if torch_installed:
            need_switch = (self.target_device == "gpu" and not current_has_cuda) or \
                          (self.target_device == "cpu" and current_has_cuda)
            if need_switch:
                self.log.emit(t("uninstall_torch_start"))
                for pkg in ["torch", "torchvision", "torchaudio"]:
                    if uninstall_package(pkg):
                        self.log.emit(t("log_pkg_removed", pkg=pkg))
                self.log.emit(t("uninstall_torch_done"))
            else:
                self.log.emit(t("torch_switch_already"))
                need_install_torch = False

        success = True
        changed = need_install_torch
        if need_install_torch:
            install_key = "installing_pytorch_gpu" if self.target_device == "gpu" else "installing_pytorch_cpu"
            self.log.emit(t(install_key))
            ok = install_dependency(
                TORCH_PACKAGES,
                index_url=TORCH_INDEX_URLS[self.target_device],
                log_callback=lambda line: self.log.emit(f"pip: {line}")
            )

            if ok:
                self.log.emit(t("pytorch_installed"))
            else:
                self.log.emit(t("install_fail", pkg="PyTorch"))
                success = False

        missing = check_transcription_deps()
        for pkg in missing:
            if pkg == "torch":
                continue
            self.log.emit(t("log_installing_pkg", pkg=pkg))
            if install_dependency(pkg, log_callback=lambda line: self.log.emit(f"pip: {line}")):
                self.log.emit(t("package_installed", pkg=pkg))
                changed = True
            else:
                self.log.emit(t("install_fail", pkg=pkg))
                success = False

        if success:
            importlib.invalidate_caches()
            try:
                import torch
                importlib.import_module("whisper")
                self.install_done.emit(True, changed, t("deps_install_success"))
            except Exception as e:
                self.install_done.emit(False, changed, f"Import failed: {e}")
        else:
            self.install_done.emit(False, changed, t("deps_install_fail"))


class ModelDownloadThread(QThread):
    log = pyqtSignal(str)
    progress = pyqtSignal(int, int)
    done = pyqtSignal(bool, str)

    def __init__(self, model_name, lang, parent=None):
        super().__init__(parent)
        self.model_name = model_name
        self.lang = lang

    def run(self):
        t = lambda key, **kw: I18n.get(key, self.lang, **kw)
        try:
            importlib.import_module("whisper")
            url = get_model_download_url(self.model_name)
            cache_dir = get_whisper_cache_dir()
            os.makedirs(cache_dir, exist_ok=True)
            dest = os.path.join(cache_dir, f"{self.model_name}.pt")
            if os.path.isfile(dest):
                self.done.emit(True, t("model_already_installed", model=self.model_name))
                return
            size_mb = get_remote_size(url) / (1024 * 1024)
            part_file = dest + ".part"
            if os.path.isfile(part_file) and os.path.getsize(part_file) > 0:
                resume_mb = os.path.getsize(part_file) / (1024 * 1024)
                self.log.emit(t("model_download_resume", model=self.model_name,
                                 size=resume_mb, total=size_mb))
            else:
                self.log.emit(t("model_download_start", model=self.model_name,
                                 size=size_mb, path=dest))

            def on_progress(pct, done_bytes, total_bytes):
                self.progress.emit(pct, 100)
                self.log.emit(t("model_download_progress", percent=pct,
                                 done=done_bytes / (1024 * 1024), total=total_bytes / (1024 * 1024)))

            download_file_single(url, dest, on_progress)
            self.done.emit(True, t("model_download_done", model=self.model_name, path=dest))
        except Exception as e:
            self.done.emit(False, t("model_download_fail", model=self.model_name, e=str(e)))
