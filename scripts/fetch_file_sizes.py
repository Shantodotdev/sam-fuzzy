#!/usr/bin/env python3
"""
Fetches file sizes for all media items in data/sam_media.json by querying
the directory listings from SamOnline h5ai web servers on the local LAN.
"""

import urllib.request
import urllib.parse
import re
import json
import time
import os
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

HEADERS = {'User-Agent': 'Mozilla/5.0'}

def format_size(raw):
    if not raw:
        return None
    raw = raw.strip()
    parts = raw.split()
    if not parts:
        return None
    try:
        val = float(parts[0])
    except ValueError:
        return raw
    unit = parts[1].upper() if len(parts) > 1 else 'KB'
    if unit == 'KB':
        bytes_val = val * 1024
    elif unit == 'MB':
        bytes_val = val * 1024 * 1024
    elif unit == 'GB':
        bytes_val = val * 1024 * 1024 * 1024
    elif unit == 'B':
        bytes_val = val
    else:
        bytes_val = val * 1024

    if bytes_val >= 1024 * 1024 * 1024:
        return f"{bytes_val / (1024 * 1024 * 1024):.2f} GB"
    elif bytes_val >= 1024 * 1024:
        return f"{bytes_val / (1024 * 1024):.1f} MB"
    elif bytes_val >= 1024:
        return f"{bytes_val / 1024:.0f} KB"
    else:
        return f"{bytes_val:.0f} B"

def format_bytes(bytes_val):
    if bytes_val >= 1024 * 1024 * 1024:
        return f"{bytes_val / (1024 * 1024 * 1024):.2f} GB"
    elif bytes_val >= 1024 * 1024:
        return f"{bytes_val / (1024 * 1024):.1f} MB"
    elif bytes_val >= 1024:
        return f"{bytes_val / 1024:.0f} KB"
    else:
        return f"{bytes_val:.0f} B"

ROW_PATTERN = re.compile(
    r'<tr>.*?<td class=\"fb-n\"><a href=\"([^\"]+)\">([^<]+)</a></td>.*?<td class=\"fb-s\">([^<]*)</td>.*?</tr>',
    re.DOTALL
)

def fetch_folder_sizes(folder_url, timeout=5):
    try:
        req = urllib.request.Request(folder_url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            html = resp.read().decode('utf-8', errors='ignore')
        
        file_sizes = {}
        for href, name, size in ROW_PATTERN.findall(html):
            fmt = format_size(size)
            if fmt:
                clean_name = name.strip()
                file_sizes[clean_name] = fmt
                href_path = urllib.parse.unquote(href)
                leaf = href_path.strip('/').split('/')[-1]
                if leaf:
                    file_sizes[leaf] = fmt
        return folder_url, file_sizes
    except Exception:
        return folder_url, {}

def main():
    json_path = os.path.join(os.path.dirname(__file__), '../data/sam_media.json')
    cache_path = os.path.join(os.path.dirname(__file__), '../data/folder_sizes_cache.json')
    
    print(f"Loading {json_path}...")
    with open(json_path, 'r', encoding='utf-8') as f:
        items = json.load(f)
    print(f"Loaded {len(items)} items.")

    # Collect unique folders
    unique_folders = sorted(list(set(it['folder_url'] for it in items if it.get('folder_url'))))
    print(f"Found {len(unique_folders)} unique folders to query.")

    # Load cache if available
    folder_cache = {}
    if os.path.exists(cache_path):
        try:
            with open(cache_path, 'r', encoding='utf-8') as f:
                folder_cache = json.load(f)
            print(f"Loaded {len(folder_cache)} folders from existing cache.")
        except Exception:
            folder_cache = {}

    to_fetch = [u for u in unique_folders if u not in folder_cache]
    print(f"Folders remaining to fetch: {len(to_fetch)}")

    t0 = time.time()
    completed = 0
    total = len(to_fetch)

    if total > 0:
        with ThreadPoolExecutor(max_workers=64) as ex:
            futures = [ex.submit(fetch_folder_sizes, u) for u in to_fetch]
            for fut in as_completed(futures):
                folder_url, sizes = fut.result()
                folder_cache[folder_url] = sizes
                completed += 1
                if completed % 2000 == 0 or completed == total:
                    elapsed = time.time() - t0
                    rate = completed / max(elapsed, 0.001)
                    print(f"  [{completed}/{total}] {completed*100/total:.1f}% ({rate:.1f} folders/s)...")
                    # Save intermediate cache
                    try:
                        with open(cache_path, 'w', encoding='utf-8') as f:
                            json.dump(folder_cache, f)
                    except Exception:
                        pass

    # Save final cache
    print("Writing folder cache to disk...")
    with open(cache_path, 'w', encoding='utf-8') as f:
        json.dump(folder_cache, f)

    # Map sizes back to items
    print("Applying sizes to items...")
    matched = 0
    for it in items:
        f_url = it.get('folder_url')
        fname = it.get('filename')
        sizes = folder_cache.get(f_url, {})
        if fname in sizes:
            it['size'] = sizes[fname]
            matched += 1
        elif fname.strip() in sizes:
            it['size'] = sizes[fname.strip()]
            matched += 1
        else:
            # Try unquoted
            unquoted = urllib.parse.unquote(fname)
            if unquoted in sizes:
                it['size'] = sizes[unquoted]
                matched += 1
            else:
                it['size'] = None

    print(f"Matched {matched}/{len(items)} items with file sizes ({matched*100/len(items):.1f}%).")

    # If any items missed size, try fallback HEAD request
    missing_items = [it for it in items if not it.get('size')]
    print(f"Items missing size: {len(missing_items)}")
    if missing_items and len(missing_items) <= 5000:
        print(f"Querying HEAD for {len(missing_items)} missing items...")
        def fetch_head_size(it):
            url = it.get('url')
            try:
                req = urllib.request.Request(url, headers=HEADERS, method='HEAD')
                with urllib.request.urlopen(req, timeout=3) as resp:
                    cl = resp.headers.get('Content-Length')
                    if cl and cl.isdigit():
                        return it, format_bytes(int(cl))
            except Exception:
                pass
            return it, None

        with ThreadPoolExecutor(max_workers=40) as ex:
            futures = [ex.submit(fetch_head_size, it) for it in missing_items]
            head_matched = 0
            for fut in as_completed(futures):
                item_ref, size_val = fut.result()
                if size_val:
                    item_ref['size'] = size_val
                    head_matched += 1
            print(f"Recovered {head_matched} sizes via HEAD request.")
            matched += head_matched

    print(f"Final match count: {matched}/{len(items)} ({matched*100/len(items):.1f}%).")

    print(f"Saving updated dataset to {json_path}...")
    with open(json_path, 'w', encoding='utf-8') as f:
        json.dump(items, f, indent=2, ensure_ascii=False)
    print("Done!")

if __name__ == '__main__':
    main()
