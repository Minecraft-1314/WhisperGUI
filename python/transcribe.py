import json
import os
import sys
import traceback


ALLOWED_EXTENSIONS = {
    "mp3", "wav", "m4a", "m4b", "mp4", "flac", "wma", "ogg", "oga", "opus",
    "aac", "aiff", "aif", "webm", "mkv", "mov", "amr", "3gp", "mka", "m4v",
}
TRANSCRIPT_SUFFIX = "_transcript"


def emit(payload):
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def read_request():
    raw = sys.stdin.read()
    if not raw:
        raise ValueError("Missing request.")
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("Invalid request.")
    files = value.get("files")
    if not isinstance(files, list) or not files or len(files) > 1000:
        raise ValueError("Invalid file list.")
    for path in files:
        if not isinstance(path, str) or not path or not os.path.isfile(path):
            raise ValueError("Invalid input file.")
        if os.path.splitext(path)[1].lower().lstrip(".") not in ALLOWED_EXTENSIONS:
            raise ValueError("Unsupported audio file.")
    output_directory = value.get("outputDirectory")
    if not isinstance(output_directory, str) or not os.path.isdir(output_directory):
        raise ValueError("Invalid output directory.")
    model = value.get("model")
    if model not in {"tiny", "base", "small", "medium", "large"}:
        raise ValueError("Invalid model.")
    model_file = value.get("modelFile")
    cache_directory = value.get("cacheDirectory")
    if not isinstance(model_file, str) or not isinstance(cache_directory, str):
        raise ValueError("Invalid model cache.")
    model_path = os.path.join(cache_directory, model_file)
    if not os.path.isfile(model_path) or os.path.getsize(model_path) <= 0:
        raise FileNotFoundError("Model file is missing.")
    device = value.get("device", "auto")
    if device not in {"auto", "cpu", "gpu"}:
        raise ValueError("Invalid device.")
    language = value.get("language", "auto")
    if language not in {"auto", "zh", "en", "ja", "ko", "fr", "de", "es", "ru"}:
        raise ValueError("Invalid language.")
    return {
        "files": [os.path.abspath(path) for path in files],
        "model": model,
        "modelFile": model_file,
        "cacheDirectory": cache_directory,
        "outputDirectory": output_directory,
        "device": device,
        "language": None if language == "auto" else language,
    }


def resolve_device(torch, requested):
    if requested == "cpu":
        return "cpu"
    if requested == "gpu":
        if torch.cuda.is_available():
            return "cuda"
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return "mps"
        return "cpu"
    if torch.cuda.is_available():
        return "cuda"
    if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def unique_output_path(directory, source_path, taken):
    stem = os.path.splitext(os.path.basename(source_path))[0] + TRANSCRIPT_SUFFIX
    candidate = os.path.join(directory, stem + ".txt")
    counter = 2
    while candidate in taken:
        candidate = os.path.join(directory, f"{stem}_{counter}.txt")
        counter += 1
    taken.add(candidate)
    return candidate


def run():
    request = read_request()
    try:
        import torch
        import whisper
    except Exception:
        emit({
            "type": "summary",
            "outputFiles": [],
            "failedFiles": request["files"],
            "cancelled": False,
            "error": "Python dependencies are not ready.",
        })
        return 2

    device = resolve_device(torch, request["device"])
    total = len(request["files"])
    emit({"type": "log", "message": f"Device: {device}"})
    try:
        model = whisper.load_model(
            request["model"],
            device=device,
            download_root=request["cacheDirectory"],
        )
    except Exception:
        emit({
            "type": "summary",
            "outputFiles": [],
            "failedFiles": request["files"],
            "cancelled": False,
            "error": "Model loading failed.",
        })
        return 3

    output_files = []
    failed_files = []
    taken = set()
    for index, path in enumerate(request["files"], start=1):
        emit({
            "type": "progress",
            "current": index - 1,
            "total": total,
            "message": os.path.basename(path),
        })
        emit({
            "type": "log",
            "message": f"Processing {index}/{total}: {path}",
        })
        try:
            result = model.transcribe(
                path,
                language=request["language"],
                fp16=device == "cuda",
                verbose=False,
            )
            text = str(result.get("text", "")).strip()
            if request["language"] is None and result.get("language"):
                emit({
                    "type": "language",
                    "current": index,
                    "total": total,
                    "message": str(result["language"]),
                    "path": path,
                })
            output_path = unique_output_path(request["outputDirectory"], path, taken)
            with open(output_path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
            output_files.append(output_path)
            emit({
                "type": "saved",
                "current": index,
                "total": total,
                "message": output_path,
                "path": output_path,
            })
            emit({
                "type": "result",
                "current": index,
                "total": total,
                "message": text,
                "path": path,
            })
        except Exception as error:
            failed_files.append(path)
            emit({
                "type": "fileFailed",
                "current": index,
                "total": total,
                "message": str(error),
                "path": path,
                "error": str(error),
            })

    emit({
        "type": "summary",
        "outputFiles": output_files,
        "failedFiles": failed_files,
        "cancelled": False,
        "error": None,
    })
    return 0


if __name__ == "__main__":
    try:
        sys.exit(run())
    except Exception:
        traceback.print_exc(file=sys.stderr)
        emit({
            "type": "summary",
            "outputFiles": [],
            "failedFiles": [],
            "cancelled": False,
            "error": "Transcription runner failed.",
        })
        sys.exit(4)
