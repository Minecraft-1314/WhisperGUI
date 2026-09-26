import importlib
import importlib.util
import os
import shutil
import subprocess
import sys

from .constants import (
    IMPORT_PROBE_TIMEOUT,
    PACKAGE_INSTALL_TIMEOUT,
    PYTHON_PROBE_TIMEOUT,
)

TRANSCRIPTION_MODULES = ("whisper", "torch")

MODULE_DISTRIBUTIONS = {
    "whisper": "openai-whisper",
    "torch": "torch",
}

TORCH_DISTRIBUTION = "torch"


def _no_window_flags():
    return subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0


def _is_valid_python(command):
    try:
        result = subprocess.run(
            command + ["-c", "import sys"],
            capture_output=True,
            timeout=PYTHON_PROBE_TIMEOUT,
            creationflags=_no_window_flags(),
        )
        return result.returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def get_python_command():
    if not getattr(sys, "frozen", False):
        return [sys.executable]
    candidates = [
        getattr(sys, "_base_executable", None),
        shutil.which("python"),
        shutil.which("python3"),
    ]
    for candidate in candidates:
        if (
            candidate
            and os.path.isfile(candidate)
            and "python" in os.path.basename(candidate).lower()
            and _is_valid_python([candidate])
        ):
            return [candidate]
    py_launcher = shutil.which("py")
    if py_launcher and os.path.isfile(py_launcher) and _is_valid_python([py_launcher, "-3"]):
        return [py_launcher, "-3"]
    return []


def _drain_output(process, log_callback, timeout):
    if log_callback:
        for line in process.stdout:
            line = line.strip()
            if line:
                log_callback(line)
        return process.wait()
    process.communicate(timeout=timeout)
    return process.returncode


def install_dependency(package, index_url=None, timeout=PACKAGE_INSTALL_TIMEOUT, log_callback=None):
    python_command = get_python_command()
    if not python_command:
        return False
    command = python_command + ["-m", "pip", "install"]
    if index_url:
        command += ["--index-url", index_url]
    command += package.split()
    try:
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            creationflags=_no_window_flags(),
        )
    except OSError:
        return False
    try:
        return _drain_output(process, log_callback, timeout) == 0
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        if log_callback:
            log_callback("pip: timed out")
        return False
    finally:
        if process.stdout:
            process.stdout.close()
        importlib.invalidate_caches()


def uninstall_package(package, timeout=PACKAGE_INSTALL_TIMEOUT):
    python_command = get_python_command()
    if not python_command:
        return False
    try:
        subprocess.run(
            python_command + ["-m", "pip", "uninstall", "-y", package],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=timeout,
            creationflags=_no_window_flags(),
        )
    except (OSError, subprocess.SubprocessError):
        return False
    importlib.invalidate_caches()
    return True


def ensure_gui_dependency():
    try:
        importlib.import_module("PyQt6")
    except ImportError:
        print("PyQt6 is required. Attempting to install...")
        if install_dependency("PyQt6"):
            print("Installation successful. Please restart the application.")
        else:
            print("Failed to install PyQt6. Please run: pip install PyQt6")
        sys.exit(1)


def find_missing_modules(modules=TRANSCRIPTION_MODULES):
    missing = []
    for name in modules:
        try:
            if importlib.util.find_spec(name) is None:
                missing.append(name)
        except (ImportError, ValueError):
            missing.append(name)
    return missing


def check_transcription_deps():
    return find_missing_modules()


def module_distribution_name(module_name):
    return MODULE_DISTRIBUTIONS.get(module_name, module_name)


def module_imports_in_subprocess(module_name):
    python_command = get_python_command()
    if not python_command:
        return False
    try:
        result = subprocess.run(
            python_command + ["-c", "import %s" % module_name],
            capture_output=True,
            timeout=IMPORT_PROBE_TIMEOUT,
            creationflags=_no_window_flags(),
        )
        return result.returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def probe_modules(modules=TRANSCRIPTION_MODULES):
    return {name: module_imports_in_subprocess(name) for name in modules}
