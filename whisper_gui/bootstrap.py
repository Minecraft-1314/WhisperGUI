import os
import shutil
import sys
import subprocess
import importlib


def _no_window_flags():
    return subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0


def _is_valid_python(command):
    try:
        result = subprocess.run(
            command + ["-c", "import sys"],
            capture_output=True,
            timeout=15,
            creationflags=_no_window_flags(),
        )
        return result.returncode == 0
    except Exception:
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


def install_dependency(package, index_url=None, upgrade=False, log_callback=None):
    try:
        python_command = get_python_command()
        if not python_command:
            return False
        cmd = python_command + ["-m", "pip", "install"]
        if upgrade:
            cmd.append("--upgrade")
        cmd.extend(package.split())
        if index_url:
            cmd.extend(["--index-url", index_url])
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            creationflags=_no_window_flags(),
        )
        if log_callback:
            for line in proc.stdout:
                line = line.strip()
                if line:
                    log_callback(line)
        else:
            proc.communicate()
        rc = proc.wait()
        importlib.invalidate_caches()
        return rc == 0
    except Exception:
        return False

def uninstall_package(package):
    try:
        python_command = get_python_command()
        if not python_command:
            return False
        subprocess.check_call(
            python_command + ["-m", "pip", "uninstall", "-y", package],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=_no_window_flags(),
        )
        importlib.invalidate_caches()
        return True
    except Exception:
        return False

def ensure_gui_dependency():
    try:
        __import__("PyQt6")
    except ImportError:
        print("PyQt6 is required. Attempting to install...")
        if install_dependency("PyQt6"):
            print("Installation successful. Please restart the application.")
        else:
            print("Failed to install PyQt6. Please run: pip install PyQt6")
        sys.exit(1)

def check_transcription_deps():
    missing = []
    for mod in ["whisper", "torch"]:
        try:
            importlib.import_module(mod)
        except ImportError:
            missing.append(mod)
    return missing
