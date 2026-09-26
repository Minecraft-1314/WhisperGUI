import os
import sys
import threading

from PyQt6.QtCore import QThread, pyqtSignal

from .bootstrap import (
    TORCH_DISTRIBUTION,
    find_missing_modules,
    install_dependency,
    module_distribution_name,
    probe_modules,
    uninstall_package,
)
from .constants import (
    CUDA_INDEX_TEMPLATE,
    TORCH_INDEX_URL,
    TORCH_INSTALL_TIMEOUT,
    TORCH_PACKAGE,
    TRANSCRIPT_SUFFIX,
)
from .download import (
    download_model_file,
    get_model_download_url,
    get_model_path,
    get_remote_size,
    get_whisper_cache_dir,
    load_whisper_model,
)
from .i18n import I18n
from .utils import LogStream, detect_cuda_variants

MEGABYTE = 1024 * 1024


def unique_output_path(directory, base_name, suffix, taken):
    stem = "%s%s" % (base_name, suffix)
    candidate = os.path.join(directory, "%s.txt" % stem)
    counter = 2
    while candidate in taken:
        candidate = os.path.join(directory, "%s_%d.txt" % (stem, counter))
        counter += 1
    taken.add(candidate)
    return candidate


class TranscriptionThread(QThread):
    status = pyqtSignal(str)
    log = pyqtSignal(str)
    result = pyqtSignal(str)
    error = pyqtSignal(str)
    file_failed = pyqtSignal(str)
    progress = pyqtSignal(int, int)

    def __init__(self, file_paths, model_name, device, output_dir, ui_lang="en",
                 transcribe_language=None, parent=None):
        super().__init__(parent)
        self.file_paths = list(file_paths)
        self.model_name = model_name
        self.device = device
        self.output_dir = output_dir
        self.ui_lang = ui_lang
        if transcribe_language in (None, "auto"):
            self.transcribe_language = None
        else:
            self.transcribe_language = transcribe_language
        self.output_paths = []
        self.failed_files = []
        self._cancel = threading.Event()
        self._streams_lock = threading.Lock()
        self._saved_stdout = None
        self._saved_stderr = None

    def request_cancel(self):
        self._cancel.set()

    def is_cancelled(self):
        return self._cancel.is_set()

    def _enter_streams(self):
        with self._streams_lock:
            self._saved_stdout = sys.stdout
            self._saved_stderr = sys.stderr
            sys.stdout = LogStream(self.log, self._saved_stdout)
            sys.stderr = LogStream(self.log, self._saved_stderr)

    def restore_streams(self):
        with self._streams_lock:
            if self._saved_stdout is not None:
                sys.stdout = self._saved_stdout
                self._saved_stdout = None
            if self._saved_stderr is not None:
                sys.stderr = self._saved_stderr
                self._saved_stderr = None

    def run(self):
        self._enter_streams()
        try:
            self._run_batch()
        except Exception as error:
            self.error.emit(str(error))
        finally:
            self.restore_streams()

    def _run_batch(self):
        self.status.emit(I18n.get("transcribing", self.ui_lang))
        model = load_whisper_model(self.model_name, self.device)
        total = len(self.file_paths)
        taken = set()
        for index, file_path in enumerate(self.file_paths, start=1):
            if self.is_cancelled():
                break
            self.progress.emit(index - 1, total)
            entry = I18n.get("log_batch_file", self.ui_lang)
            self.log.emit(entry.format(current=index, total=total, path=file_path))
            try:
                self._transcribe_one(model, file_path, index, total, taken)
            except Exception as error:
                self.failed_files.append((file_path, str(error)))
                entry = I18n.get("log_file_failed", self.ui_lang)
                self.file_failed.emit(entry.format(path=file_path, e=str(error)))
        if not self.is_cancelled():
            self.progress.emit(total, total)

    def _transcribe_one(self, model, file_path, index, total, taken):
        result = model.transcribe(
            file_path,
            language=self.transcribe_language,
            fp16=(self.device == "cuda"),
            verbose=True,
        )
        text = result["text"].strip()
        if self.transcribe_language is None:
            self._log_detected_language(result)
        base_name = os.path.splitext(os.path.basename(file_path))[0]
        out_path = unique_output_path(self.output_dir, base_name, TRANSCRIPT_SUFFIX, taken)
        with open(out_path, "w", encoding="utf-8") as handle:
            handle.write(text)
        self.output_paths.append(out_path)
        header = "[%d/%d] %s\n" % (index, total, os.path.basename(file_path))
        self.result.emit("%s%s\n" % (header, text))
        self.log.emit(I18n.get("log_saved_to", self.ui_lang, path=out_path))

    def _log_detected_language(self, result):
        detected = result.get("language")
        if detected:
            entry = I18n.get("log_language_detected", self.ui_lang)
            self.log.emit(entry.format(language=detected))


class InstallThread(QThread):
    log = pyqtSignal(str)
    install_done = pyqtSignal(bool, bool, str)

    def __init__(self, target_device, lang, parent=None):
        super().__init__(parent)
        self.target_device = target_device
        self.lang = lang
        self.changed = False

    def _t(self, key, **kwargs):
        return I18n.get(key, self.lang, **kwargs)

    def _emit_pip(self, line):
        self.log.emit("pip: %s" % line)

    @staticmethod
    def _torch_installed():
        return "torch" not in find_missing_modules(("torch",))

    @staticmethod
    def _torch_has_cuda():
        try:
            import torch
            return bool(torch.cuda.is_available())
        except ImportError:
            return False

    def _uninstall_torch(self):
        self.log.emit(self._t("uninstall_torch_start"))
        for package in (TORCH_DISTRIBUTION, "torchvision", "torchaudio"):
            if uninstall_package(package):
                self.log.emit(self._t("log_pkg_removed", pkg=package))
        self.log.emit(self._t("uninstall_torch_done"))

    def _install_cpu_torch(self):
        self.log.emit(self._t("installing_pytorch_cpu"))
        return install_dependency(
            TORCH_PACKAGE,
            index_url=TORCH_INDEX_URL,
            timeout=TORCH_INSTALL_TIMEOUT,
            log_callback=self._emit_pip,
        )

    def _install_gpu_torch(self):
        candidates = detect_cuda_variants()
        if not candidates:
            self.log.emit(self._t("install_fail", pkg="PyTorch"))
            return False
        for position, variant in enumerate(candidates):
            if self._install_cuda_torch(variant):
                return True
            if position == len(candidates) - 1:
                return False
        return False

    def _install_cuda_torch(self, variant):
        label = variant.upper()[2:]
        self.log.emit(self._t("installing_pytorch_gpu", variant=label))
        index_url = CUDA_INDEX_TEMPLATE.format(variant=variant)
        installed = install_dependency(
            TORCH_PACKAGE,
            index_url=index_url,
            timeout=TORCH_INSTALL_TIMEOUT,
            log_callback=self._emit_pip,
        )
        if not installed:
            self.log.emit(self._t("log_cuda_variant_failed", variant=label))
        return installed

    def _install_torch(self):
        if self.target_device == "cpu":
            return self._install_cpu_torch()
        return self._install_gpu_torch()

    def _install_missing_modules(self):
        success = True
        for module_name in find_missing_modules():
            if module_name == "torch":
                continue
            package = module_distribution_name(module_name)
            self.log.emit(self._t("log_installing_pkg", pkg=package))
            if install_dependency(package, log_callback=self._emit_pip):
                self.log.emit(self._t("package_installed", pkg=package))
                self.changed = True
            else:
                self.log.emit(self._t("install_fail", pkg=package))
                success = False
        return success

    def run(self):
        need_install_torch = True
        if self._torch_installed():
            need_switch = (self.target_device == "gpu") != self._torch_has_cuda()
            if need_switch:
                self._uninstall_torch()
            else:
                self.log.emit(self._t("torch_switch_already"))
                need_install_torch = False

        success = True
        if need_install_torch:
            if self._install_torch():
                self.log.emit(self._t("pytorch_installed"))
                self.changed = True
            else:
                self.log.emit(self._t("install_fail", pkg="PyTorch"))
                success = False

        if not self._install_missing_modules():
            success = False

        if not success:
            self.install_done.emit(False, self.changed, self._t("deps_install_fail"))
            return

        broken = self._broken_modules()
        if broken:
            message = self._t("deps_verify_failed", list=", ".join(broken))
            self.install_done.emit(False, self.changed, message)
            return
        self.install_done.emit(True, self.changed, self._t("deps_install_success"))

    @staticmethod
    def _broken_modules():
        return [name for name, ok in probe_modules().items() if not ok]


class ModelDownloadThread(QThread):
    log = pyqtSignal(str)
    progress = pyqtSignal(int, int)
    done = pyqtSignal(bool, str)

    def __init__(self, model_name, lang, parent=None):
        super().__init__(parent)
        self.model_name = model_name
        self.lang = lang
        self._cancel = threading.Event()

    def request_cancel(self):
        self._cancel.set()

    def is_cancelled(self):
        return self._cancel.is_set()

    def _t(self, key, **kwargs):
        return I18n.get(key, self.lang, **kwargs)

    def _fail(self, reason):
        entry = self._t("model_download_fail", model=self.model_name)
        self.done.emit(False, entry.format(e=reason))

    def run(self):
        try:
            self._run_download()
        except Exception as error:
            self._fail(str(error))

    def _run_download(self):
        url = get_model_download_url(self.model_name)
        dest = get_model_path(self.model_name)
        os.makedirs(get_whisper_cache_dir(), exist_ok=True)
        total_size = get_remote_size(url)
        if total_size <= 0:
            self._fail(self._t("model_size_unknown"))
            return
        if self._cached_file_is_usable(dest, total_size):
            message = self._t("model_already_installed", model=self.model_name)
            self.done.emit(True, message)
            return
        self._log_start(url, dest, total_size)
        self._download(url, dest, total_size)
        message = self._t("model_download_done", model=self.model_name, path=dest)
        self.done.emit(True, message)

    @staticmethod
    def _cached_file_is_usable(dest, total_size):
        if not os.path.isfile(dest):
            return False
        if os.path.getsize(dest) != total_size:
            os.remove(dest)
            return False
        return True

    def _log_start(self, url, dest, total_size):
        size_mb = total_size / MEGABYTE
        part_file = dest + ".part"
        if os.path.isfile(part_file) and os.path.getsize(part_file) > 0:
            resume_mb = os.path.getsize(part_file) / MEGABYTE
            entry = self._t("model_download_resume", model=self.model_name)
            self.log.emit(entry.format(size=resume_mb, total=size_mb))
            return
        entry = self._t("model_download_start", model=self.model_name)
        self.log.emit(entry.format(size=size_mb, path=dest))

    def _download(self, url, dest, total_size):
        def on_progress(percent, done_bytes, total_bytes):
            if self.is_cancelled():
                raise RuntimeError(self._t("download_cancelled"))
            self.progress.emit(percent, 100)
            entry = self._t("model_download_progress", percent=percent)
            done_mb = done_bytes / MEGABYTE
            self.log.emit(entry.format(done=done_mb, total=total_bytes / MEGABYTE))

        download_model_file(url, dest, on_progress, total_size)
