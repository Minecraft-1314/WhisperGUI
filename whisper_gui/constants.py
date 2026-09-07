WHISPER_MODELS = ("tiny", "base", "small", "medium", "large")
MODEL_REFERENCE_SIZES_MB = {
    "tiny": 75,
    "base": 142,
    "small": 466,
    "medium": 1540,
    "large": 3090,
}
TORCH_INDEX_URLS = {
    "cpu": "https://download.pytorch.org/whl/cpu",
    "gpu": "https://download.pytorch.org/whl/cu118",
}
TORCH_PACKAGES = "torch torchvision torchaudio"
DOWNLOAD_CHUNK_SIZE = 1024 * 256
REQUEST_TIMEOUT = 120
HEAD_TIMEOUT = 30
DEVICE_CHOICES = ("auto", "cpu", "gpu")
SUPPORTED_LANGUAGES = ("auto", "zh", "en", "ja", "ko", "fr", "de", "es", "ru")
MODEL_CACHE = {}
