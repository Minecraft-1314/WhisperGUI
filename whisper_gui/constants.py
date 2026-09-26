WHISPER_MODELS = ("tiny", "base", "small", "medium", "large")

MODEL_REFERENCE_SIZES_MB = {
    "tiny": 75,
    "base": 142,
    "small": 466,
    "medium": 1540,
    "large": 3090,
}

TORCH_INDEX_URL = "https://download.pytorch.org/whl/cpu"

CUDA_INDEX_TEMPLATE = "https://download.pytorch.org/whl/{variant}"

CUDA_VARIANTS = ("cu128", "cu126", "cu124", "cu121", "cu118")

CUDA_MIN_DRIVER = {
    "cu128": (570, 0),
    "cu126": (560, 0),
    "cu124": (550, 0),
    "cu121": (530, 0),
    "cu118": (520, 0),
}

TORCH_PACKAGE = "torch"

TORCH_INSTALL_TIMEOUT = 3600
PACKAGE_INSTALL_TIMEOUT = 1800
PYTHON_PROBE_TIMEOUT = 20
IMPORT_PROBE_TIMEOUT = 900

DOWNLOAD_CHUNK_SIZE = 1024 * 256
REQUEST_TIMEOUT = 120
HEAD_TIMEOUT = 30

DEVICE_CHOICES = ("auto", "cpu", "gpu")
SUPPORTED_LANGUAGES = ("auto", "zh", "en", "ja", "ko", "fr", "de", "es", "ru")

AUDIO_EXTENSIONS = (
    "mp3", "wav", "m4a", "m4b", "mp4", "flac", "wma", "ogg", "oga", "opus",
    "aac", "aiff", "aif", "webm", "mkv", "mov", "amr", "3gp", "mka", "m4v",
)

TRANSCRIPT_SUFFIX = "_transcript"

MODEL_CACHE = {}

LARGE_MODEL_MIN_GB = 10
