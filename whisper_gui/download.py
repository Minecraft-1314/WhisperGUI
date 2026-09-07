import os
import urllib.request
from urllib.error import HTTPError, URLError

from .constants import DOWNLOAD_CHUNK_SIZE, HEAD_TIMEOUT, MODEL_CACHE, REQUEST_TIMEOUT


def get_model_download_url(model_name):
    import whisper
    url = getattr(whisper, "_MODELS", {}).get(model_name)
    if not url:
        raise ValueError(f"Unknown model: {model_name}")
    return url


def get_whisper_cache_dir():
    return os.path.join(os.path.expanduser("~"), ".cache", "whisper")


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
        req = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(req, timeout=HEAD_TIMEOUT) as resp:
            return int(resp.headers.get("Content-Length", 0))
    except (HTTPError, URLError, OSError):
        with urllib.request.urlopen(url, timeout=HEAD_TIMEOUT) as resp:
            return int(resp.headers.get("Content-Length", 0))

def download_file_single(url, dest, progress_cb=None):
    total = get_remote_size(url)
    if total <= 0:
        raise ValueError("Cannot determine file size")
    part_file = dest + ".part"
    downloaded = 0
    mode = "wb"
    headers = {}
    if os.path.isfile(part_file):
        downloaded = os.path.getsize(part_file)
        if downloaded > 0 and downloaded < total:
            mode = "ab"
            headers["Range"] = f"bytes={downloaded}-"
        elif downloaded > total:
            downloaded = 0
        elif downloaded == total:
            os.replace(part_file, dest)
            return total
    req = urllib.request.Request(url, headers=headers)
    try:
        resp = urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT)
    except HTTPError as exc:
        if exc.code != 416:
            raise
        downloaded = 0
        mode = "wb"
        resp = urllib.request.urlopen(url, timeout=REQUEST_TIMEOUT)
    else:
        if headers and getattr(resp, "status", 200) != 206:
            resp.close()
            downloaded = 0
            mode = "wb"
            resp = urllib.request.urlopen(url, timeout=REQUEST_TIMEOUT)
    last_pct = -1
    with resp, open(part_file, mode) as f:
        while True:
            chunk = resp.read(DOWNLOAD_CHUNK_SIZE)
            if not chunk:
                break
            f.write(chunk)
            downloaded += len(chunk)
            if progress_cb:
                pct = int(downloaded / total * 100)
                if pct != last_pct and pct % 10 == 0:
                    last_pct = pct
                    progress_cb(pct, downloaded, total)
    if os.path.getsize(part_file) != total:
        raise ValueError("Downloaded size mismatch")
    os.replace(part_file, dest)
    return total
