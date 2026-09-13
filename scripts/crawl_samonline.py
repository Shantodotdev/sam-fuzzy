#!/usr/bin/env python3
"""
High-speed concurrent crawler for SamOnline FTP mirrors (DhakaFlix).
Collects ONLY direct video files (.mkv, .mp4, .avi, .webm, .flv, .wmv).
Extracts resolution (1080p, 720p, 2160p, 480p) STRICTLY from the filename.
Excludes all directory / folder entries.
"""

import urllib.request
import urllib.parse
import re
import json
import os
import sys
import time
import sqlite3
from concurrent.futures import ThreadPoolExecutor, as_completed

HEADERS = {'User-Agent': 'Mozilla/5.0'}
VIDEO_EXTS = ('.mkv', '.mp4', '.avi', '.flv', '.wmv', '.vob', '.webm')
SKIP_EXTS = {'.jpg', '.jpeg', '.png', '.gif', '.svg', '.css', '.js', '.srt', '.sub', '.idx', '.nfo', '.txt', '.torrent', '.html', '.php'}

def fetch_dir(url, timeout=4):
    try:
        req = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            html = resp.read().decode('utf-8', errors='ignore')
        entries = []
        for href, text in re.findall(r'<a href=\"([^\"]+)\">([^<]+)</a>', html):
            if href.startswith('?') or href.startswith('/_h5ai') or href == '..' or href.startswith('http'):
                continue
            full_url = urllib.parse.urljoin(url, href)
            is_dir = href.endswith('/')
            entries.append((full_url, is_dir, text.strip()))
        return entries
    except Exception:
        return []

def clean_video_filename(filename):
    """
    Extracts title, year, and quality profile STRICTLY from the filename.
    No resolution or codec tags are inherited from parent folder names.
    """
    base = filename
    if '.' in base:
        base = base[:base.rfind('.')]

    # 1. Quality & Resolution tokens - STRICTLY FROM FILENAME
    qualities = []
    tag_patterns = [
        ('2160p', r'\b(2160p|4k|uhd)\b'),
        ('1080p', r'\b1080p\b'),
        ('720p', r'\b720p\b'),
        ('480p', r'\b480p\b'),
        ('BluRay', r'\b(bluray|bdrip|brrip)\b'),
        ('WEBRip', r'\b(webrip|web-dl|webdl)\b'),
        ('HDRip', r'\bhdrip\b'),
        ('DVDRip', r'\bdvdrip\b'),
        ('HDTV', r'\bhdtv\b'),
        ('Dual Audio', r'\b(dual[\s._-]?audio)\b'),
        ('Multi Audio', r'\b(multi[\s._-]?audio)\b'),
        ('3D', r'\b3d\b'),
        ('HEVC', r'\b(hevc|x265)\b'),
        ('x264', r'\bx264\b'),
    ]
    for label, pat in tag_patterns:
        if re.search(pat, filename, re.IGNORECASE):
            if label not in qualities:
                qualities.append(label)

    quality = ' / '.join(qualities) if qualities else 'Standard'

    # 2. Release Year detection (1920 - 2029)
    year = None
    m_year = re.search(r'[\(\[\._\s](19\d\d|20\d\d)[\)\]\._\s]', filename)
    if m_year:
        year = int(m_year.group(1))

    # 3. Clean human-readable title
    clean = base.replace('.', ' ').replace('_', ' ')

    # Check for Episode identifiers: S01E01, S1, EP01, etc.
    m_ep = re.search(r'\b(S\d+E\d+|S\d+|EP\d+|E\d+)\b', clean, re.IGNORECASE)

    if m_ep:
        ep_code = m_ep.group(1).upper()
        idx = clean.upper().find(ep_code)
        title_part = clean[:idx].strip(' -_()[]')
        title = f'{title_part} {ep_code}'.strip()
    elif year:
        idx = clean.find(str(year))
        if idx > 0:
            title = clean[:idx].strip(' -_()[]')
        else:
            title = clean
    else:
        title = re.sub(
            r'\b(720p|1080p|2160p|4k|bluray|webrip|web-dl|hdrip|dvdrip|hdtv|x264|x265|hevc|aac|ac3|dts|remux).*',
            '', clean, flags=re.IGNORECASE
        )
        title = title.strip(' -_()[]')

    title = re.sub(r'\[.*?\]|\(.*?\)', '', title)
    title = re.sub(r'\s+', ' ', title).strip(' -_')
    if not title or len(title) < 2:
        title = base

    return title, year, quality

def parse_direct_video_item(url, category, server):
    parsed = urllib.parse.urlparse(url)
    path = urllib.parse.unquote(parsed.path).strip('/')
    parts = path.split('/')
    file_name = parts[-1]

    ext = '.' + file_name.split('.')[-1].lower() if '.' in file_name else ''
    if ext not in VIDEO_EXTS:
        return None

    title, year, quality = clean_video_filename(file_name)
    folder_url = url[:url.rfind('/') + 1]

    return {
        'title': title,
        'year': year,
        'quality': quality,
        'category': category,
        'filename': file_name,
        'is_file': True,
        'url': url,
        'folder_url': folder_url,
        'server': server,
        'path': path
    }

def crawl_folder_recursive(url, category, server, max_depth=3):
    """Recursively crawls directories up to max_depth and returns direct video files."""
    results = []
    stack = [(url, 0)]
    while stack:
        curr_url, depth = stack.pop()
        entries = fetch_dir(curr_url)
        for entry_url, is_dir, name in entries:
            if is_dir:
                if depth < max_depth:
                    stack.append((entry_url, depth + 1))
            else:
                if any(entry_url.lower().endswith(ext) for ext in VIDEO_EXTS):
                    item = parse_direct_video_item(entry_url, category, server)
                    if item:
                        results.append(item)
    return results

def main():
    print("=== SamOnline Direct Video File Crawler ===")
    t_start = time.time()
    all_files = []
    seen_urls = set()

    # --- 1. Load Blacksparrow direct files from 172.16.50.7 ---
    db_path = os.path.expanduser('~/.local/share/blacksparrow/blacksparrow.db')
    if os.path.exists(db_path):
        print("Reading 172.16.50.7 direct video files from blacksparrow.db...")
        conn = sqlite3.connect(db_path)
        c = conn.cursor()
        c.execute("SELECT DISTINCT target_url FROM links WHERE crawl_id = 'crawl_1789221519881124'")
        bs_urls = [r[0] for r in c.fetchall()]
        bs_videos = [u for u in bs_urls if any(u.lower().endswith(ext) for ext in VIDEO_EXTS)]
        print(f"Found {len(bs_videos)} video files in Blacksparrow DB for 172.16.50.7.")

        for url in bs_videos:
            if url in seen_urls:
                continue
            # Categorize based on path
            cat = "English Movies"
            if "Foreign Language Movies" in url:
                if "Korean" in url:
                    cat = "Foreign Language Movies / Korean"
                elif "Chinese" in url:
                    cat = "Foreign Language Movies / Chinese"
                elif "Japanese" in url:
                    cat = "Foreign Language Movies / Japanese"
                else:
                    cat = "Foreign Language Movies"
            elif "Kolkata Bangla" in url:
                cat = "Kolkata Bangla Movies"
            elif "3D Movies" in url:
                cat = "3D Movies"

            item = parse_direct_video_item(url, cat, "DHAKA-FLIX-7")
            if item:
                all_files.append(item)
                seen_urls.add(url)

    print(f"Loaded {len(all_files)} files from 172.16.50.7. Now crawling 172.16.50.14...")

    # --- 2. Crawl 172.16.50.14 Sections ---
    # A. Year-nested movies
    nested_sections_14 = [
        ('http://172.16.50.14/DHAKA-FLIX-14/English%20Movies%20%281080p%29/', 'English Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies/', 'Animation Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/', 'Hindi Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/South%20Movies/', 'South Indian Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/Hindi%20Dubbed/', 'South Indian Movies / Hindi Dubbed', 'DHAKA-FLIX-14'),
    ]

    for root_url, cat, srv in nested_sections_14:
        print(f"Fetching subfolders for {cat}...")
        sub_dirs = fetch_dir(root_url)
        print(f"  {len(sub_dirs)} subfolders found in {cat}.")

        def process_sub(sub_tuple):
            s_url, _, _ = sub_tuple
            return crawl_folder_recursive(s_url, cat, srv, max_depth=2)

        with ThreadPoolExecutor(max_workers=30) as ex:
            futures = [ex.submit(process_sub, s) for s in sub_dirs]
            for fut in as_completed(futures):
                try:
                    items = fut.result()
                    for it in items:
                        if it['url'] not in seen_urls:
                            seen_urls.add(it['url'])
                            all_files.append(it)
                except Exception:
                    pass

    # B. Flat movie collections on 172.16.50.14
    flat_sections_14 = [
        ('http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies%20%281080p%29/', 'Animation Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/IMDb%20Top-250%20Movies/', 'IMDb Top-250', 'DHAKA-FLIX-14'),
    ]
    for root_url, cat, srv in flat_sections_14:
        print(f"Fetching collection: {cat}...")
        movie_dirs = fetch_dir(root_url)
        print(f"  {len(movie_dirs)} movies in {cat}.")

        def process_movie(m_tuple):
            m_url, is_dir, _ = m_tuple
            if is_dir:
                return crawl_folder_recursive(m_url, cat, srv, max_depth=1)
            elif any(m_url.lower().endswith(ext) for ext in VIDEO_EXTS):
                it = parse_direct_video_item(m_url, cat, srv)
                return [it] if it else []
            return []

        with ThreadPoolExecutor(max_workers=30) as ex:
            futures = [ex.submit(process_movie, m) for m in movie_dirs]
            for fut in as_completed(futures):
                try:
                    for it in fut.result():
                        if it['url'] not in seen_urls:
                            seen_urls.add(it['url'])
                            all_files.append(it)
                except Exception:
                    pass

    # C. Korean TV & Web Series on 172.16.50.14
    korean_root = 'http://172.16.50.14/DHAKA-FLIX-14/KOREAN%20TV%20&%20WEB%20Series/'
    print("Fetching Korean TV & Web Series list...")
    korean_series = fetch_dir(korean_root)
    print(f"  Found {len(korean_series)} Korean series.")

    def process_korean(s_tuple):
        s_url, _, _ = s_tuple
        return crawl_folder_recursive(s_url, 'Korean TV & Web Series', 'DHAKA-FLIX-14', max_depth=3)

    with ThreadPoolExecutor(max_workers=40) as ex:
        futures = [ex.submit(process_korean, s) for s in korean_series]
        for fut in as_completed(futures):
            try:
                for it in fut.result():
                    if it['url'] not in seen_urls:
                        seen_urls.add(it['url'])
                        all_files.append(it)
            except Exception:
                pass

    print(f"Total direct files after 172.16.50.14: {len(all_files)}")

    # --- 3. Crawl 172.16.50.12 TV Series ---
    tv_sections_12 = [
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%98%85%20%200%20%20%E2%80%94%20%209/', 'TV Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A5%20%20A%20%20%E2%80%94%20%20L/', 'TV Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A6%20%20M%20%20%E2%80%94%20%20R/', 'TV Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A0%20%20S%20%20%E2%80%94%20%20Z/', 'TV Series', 'DHAKA-FLIX-12'),
    ]

    print("Crawling 172.16.50.12 TV-WEB-Series (0-9, A-L, M-R, S-Z)...")
    all_tv_series = []
    for tv_root, cat, srv in tv_sections_12:
        s_list = fetch_dir(tv_root)
        print(f"  {len(s_list)} series in {tv_root.split('/')[-2]}")
        for s in s_list:
            all_tv_series.append((s[0], cat, srv))

    def process_tv(tv_tuple):
        s_url, cat, srv = tv_tuple
        return crawl_folder_recursive(s_url, cat, srv, max_depth=3)

    with ThreadPoolExecutor(max_workers=50) as ex:
        futures = [ex.submit(process_tv, t) for t in all_tv_series]
        for fut in as_completed(futures):
            try:
                for it in fut.result():
                    if it['url'] not in seen_urls:
                        seen_urls.add(it['url'])
                        all_files.append(it)
            except Exception:
                pass

    print(f"\nCompleted crawl in {time.time()-t_start:.1f}s.")
    print(f"Total DIRECT VIDEO FILES collected: {len(all_files)}")

    # Re-index unique IDs
    for idx, item in enumerate(all_files, 1):
        item['id'] = idx

    # Save to data/sam_media.json
    out_path = 'data/sam_media.json'
    print(f"Writing dataset to {out_path}...")
    with open(out_path, 'w') as f:
        json.dump(all_files, f, indent=2)

    print(f"Done! Saved {len(all_files)} direct video files to {out_path}.")

if __name__ == '__main__':
    main()
