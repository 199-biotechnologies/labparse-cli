#!/usr/bin/env python3
"""
labparse-vision: Optimized lab PDF/image extraction via Qwen3.5-9B
Three optimizations applied:
1. Image downscaling (300→150 DPI = 4x fewer vision tokens)
2. Rapid-MLX prefix caching (prompt cached across pages)
3. No-think mode (structured extraction, no reasoning overhead)
"""

import sys, json, time, subprocess, tempfile, os
from pathlib import Path

try:
    import fitz  # pymupdf
except ImportError:
    print("pip install pymupdf", file=sys.stderr)
    sys.exit(1)

from PIL import Image

# Optimized DPI - lab text is large, 150 DPI is plenty readable
TARGET_DPI = 150

EXTRACTION_PROMPT = """/no_think
Extract ALL biomarkers from this lab report page. Output ONLY a valid JSON array.
Each object must have exactly these fields: name, value, unit, reference_range.
Rules:
- value must be a number (float or int), not a string
- unit must be the exact unit shown (e.g., mmol/L, g/L, IU/L, X10^9/L)
- reference_range must be the range shown (e.g., "4.3 - 5.4")
- If a value is flagged/highlighted as abnormal, add "flagged": true
- Do NOT include section headers (e.g., "Kidney Function", "Liver Function")
- Do NOT skip any biomarker row
Output ONLY the JSON array, nothing else."""


def pdf_to_images(pdf_path: str, dpi: int = TARGET_DPI) -> list[str]:
    """Convert PDF pages to optimized PNG images."""
    doc = fitz.open(pdf_path)
    paths = []
    for i in range(len(doc)):
        page = doc[i]
        pix = page.get_pixmap(dpi=dpi)
        out = tempfile.mktemp(suffix=f"_page{i}.png")
        pix.save(out)
        paths.append(out)
        print(f"  Page {i}: {pix.width}x{pix.height} ({dpi} DPI)", file=sys.stderr)
    doc.close()
    return paths


def image_to_optimized(img_path: str, max_width: int = 1240) -> str:
    """Downscale image if needed (for screenshots/photos)."""
    img = Image.open(img_path)
    if img.width > max_width:
        ratio = max_width / img.width
        new_size = (max_width, int(img.height * ratio))
        img = img.resize(new_size, Image.LANCZOS)
        out = tempfile.mktemp(suffix=".png")
        img.save(out, optimize=True)
        print(f"  Resized: {img.width}x{img.height}", file=sys.stderr)
        return out
    return img_path


def extract_via_rapid_mlx(image_path: str) -> dict:
    """Extract biomarkers using Rapid-MLX server (prefix caching enabled)."""
    import base64
    img_data = base64.b64encode(open(image_path, "rb").read()).decode()
    
    payload = {
        "model": "qwen3.5-9b",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{img_data}"}},
                {"type": "text", "text": EXTRACTION_PROMPT}
            ]
        }],
        "max_tokens": 2000,
        "temperature": 0.0
    }
    
    import urllib.request
    req = urllib.request.Request(
        "http://localhost:8000/v1/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"}
    )
    
    start = time.time()
    resp = urllib.request.urlopen(req, timeout=300)
    data = json.loads(resp.read())
    elapsed = time.time() - start
    
    content = data["choices"][0]["message"]["content"]
    usage = data.get("usage", {})
    
    # Parse JSON from response (strip markdown fences if present)
    content = content.strip()
    if content.startswith("```"):
        content = content.split("\n", 1)[1].rsplit("```", 1)[0].strip()
    
    try:
        biomarkers = json.loads(content)
    except json.JSONDecodeError:
        biomarkers = []
    
    return {
        "biomarkers": biomarkers,
        "elapsed_s": round(elapsed, 1),
        "prompt_tokens": usage.get("prompt_tokens", 0),
        "completion_tokens": usage.get("completion_tokens", 0)
    }


def extract_via_ollama(image_path: str) -> dict:
    """Fallback: extract via Ollama if Rapid-MLX not running."""
    start = time.time()
    result = subprocess.run(
        ["ollama", "run", "qwen3.5:9b", EXTRACTION_PROMPT, image_path],
        capture_output=True, text=True, timeout=300
    )
    elapsed = time.time() - start
    
    content = result.stdout.strip()
    if content.startswith("```"):
        content = content.split("\n", 1)[1].rsplit("```", 1)[0].strip()
    
    try:
        biomarkers = json.loads(content)
    except json.JSONDecodeError:
        biomarkers = []
    
    return {"biomarkers": biomarkers, "elapsed_s": round(elapsed, 1)}


def main():
    if len(sys.argv) < 2:
        print("Usage: labparse-vision <pdf_or_image> [--rapid|--ollama]", file=sys.stderr)
        sys.exit(2)
    
    input_path = sys.argv[1]
    backend = "rapid" if "--ollama" not in sys.argv else "ollama"
    
    # Check if Rapid-MLX is running
    if backend == "rapid":
        try:
            import urllib.request
            urllib.request.urlopen("http://localhost:8000/v1/models", timeout=2)
        except:
            print("Rapid-MLX not running, falling back to Ollama", file=sys.stderr)
            backend = "ollama"
    
    # Convert PDF to images or optimize image
    if input_path.lower().endswith(".pdf"):
        print(f"Converting PDF at {TARGET_DPI} DPI (optimized)...", file=sys.stderr)
        images = pdf_to_images(input_path)
    else:
        images = [image_to_optimized(input_path)]
    
    # Extract from each page
    all_biomarkers = []
    total_time = 0
    
    for img in images:
        print(f"Extracting: {os.path.basename(img)} via {backend}...", file=sys.stderr)
        
        if backend == "rapid":
            result = extract_via_rapid_mlx(img)
        else:
            result = extract_via_ollama(img)
        
        all_biomarkers.extend(result["biomarkers"])
        total_time += result["elapsed_s"]
        print(f"  Found {len(result['biomarkers'])} biomarkers in {result['elapsed_s']}s", file=sys.stderr)
    
    # Deduplicate by name
    seen = set()
    unique = []
    for bm in all_biomarkers:
        key = bm.get("name", "")
        if key not in seen:
            seen.add(key)
            unique.append(bm)
    
    # Output
    output = {
        "version": "1",
        "status": "success",
        "data": {
            "source": input_path,
            "biomarkers": unique,
            "pages_processed": len(images),
            "total_time_s": round(total_time, 1)
        },
        "metadata": {
            "backend": backend,
            "dpi": TARGET_DPI,
            "markers_found": len(unique),
            "optimizations": ["150dpi_downscale", "prefix_caching", "no_think_mode"]
        }
    }
    
    print(json.dumps(output, indent=2))
    
    # Cleanup temp files
    for img in images:
        if img.startswith(tempfile.gettempdir()):
            os.unlink(img)


if __name__ == "__main__":
    main()
