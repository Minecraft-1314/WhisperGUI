import hashlib
import os
import urllib.request
from urllib.error import HTTPError, URLError

from .constants import DOWNLOAD_CHUNK_SIZE, HEAD_TIMEOUT, MODEL_CACHE, REQUEST_TIMEOUT


def get_model_download_url(model_name):
    import whisper
    url = getattr(whisper, "_MODELS", {}).get(model_name)
    if not url:
        raise ValueError("Unknown model: %s" % model_name)
    return url


def get_expected_sha256(url):
    return url.split("/")[-2]


def get_whisper_cache_dir():
    default_root = os.path.join(os.path.expanduser("~"), ".cache")
    return os.path.join(os.getenv("XDG_CACHE_HOME", default_root), "whisper")


def get_model_path(model_name):
    return os.path.join(get_whisper_cache_dir(), "%s.pt" % model_name)


def is_model_cached(model_name):
    return os.path.isfile(get_model_path(model_name))


def get_cached_model_size_mb(model_name):
    try:
        return os.path.getsize(get_model_path(model_name)) / (1024 * 1024)
    except OSError:
        return 0.0


def clear_model_cache():
    MODEL_CACHE.clear()


def load_whisper_model(model_name, device):
    import whisper
    key = (model_name, device)
    model = MODEL_CACHE.get(key)
    if model is not None:
        return model
    MODEL_CACHE.clear()
    if device == "cuda":
        import torch
        torch.cuda.empty_cache()
    model = whisper.load_model(model_name, device=device)
    MODEL_CACHE[key] = model
    return model


def get_remote_size(url):
    try:
        request = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(request, timeout=HEAD_TIMEOUT) as response:
            return int(response.headers.get("Content-Length", 0))
    except (HTTPError, URLError, OSError, ValueError):
        pass
    try:
        with urllib.request.urlopen(url, timeout=HEAD_TIMEOUT) as response:
            return int(response.headers.get("Content-Length", 0))
    except (HTTPError, URLError, OSError, ValueError):
        return 0


def _sha256_of_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(DOWNLOAD_CHUNK_SIZE), b""):
            digest.update(block)
    return digest.hexdigest()


def _open_stream(url, headers, timeout):
    request = urllib.request.Request(url, headers=headers)
    try:
        return urllib.request.urlopen(request, timeout=timeout), None
    except HTTPError as error:
        if error.code != 416:
            raise
        return urllib.request.urlopen(url, timeout=timeout), 416


def download_file_single(url, dest, progress_cb=None, total_size=None):
    total = total_size if total_size and total_size > 0 else get_remote_size(url)
    if total <= 0:
        raise ValueError("Cannot determine file size")
    part_file = dest + ".part"
    downloaded = 0
    mode = "wb"
    headers = {}
    if os.path.isfile(part_file):
        existing = os.path.getsize(part_file)
        if 0 < existing < total:
            downloaded = existing
            mode = "ab"
            headers["Range"] = "bytes=%d-" % downloaded
        elif existing == total:
            os.replace(part_file, dest)
            if progress_cb:
                progress_cb(100, total, total)
            return total
        elif existing > total:
            os.remove(part_file)
    response, status = _open_stream(url, headers, REQUEST_TIMEOUT)
    if status == 416 or (headers and getattr(response, "status", 200) != 206):
        response.close()
        downloaded = 0
        mode = "wb"
        response = urllib.request.urlopen(url, timeout=REQUEST_TIMEOUT)
    last_percent = -1
    with response, open(part_file, mode) as handle:
        while True:
            chunk = response.read(DOWNLOAD_CHUNK_SIZE)
            if not chunk:
                break
            handle.write(chunk)
            downloaded += len(chunk)
            if progress_cb:
                percent = min(100, int(downloaded / total * 100))
                if percent != last_percent:
                    last_percent = percent
                    progress_cb(percent, downloaded, total)
    if os.path.getsize(part_file) != total:
        raise ValueError("Downloaded size mismatch")
    os.replace(part_file, dest)
    return total


def download_model_file(url, dest, progress_cb=None, total_size=None):
    total = download_file_single(url, dest, progress_cb, total_size)
    expected = get_expected_sha256(url)
    if _sha256_of_file(dest) != expected:
        os.remove(dest)
        raise ValueError("Checksum mismatch")
    return total
