import urllib.request, re, urllib.parse, json, os, sys
from concurrent.futures import ThreadPoolExecutor, as_completed

headers = {'User-Agent': 'Mozilla/5.0'}

skip_exts = {'.jpg', '.jpeg', '.png', '.gif', '.svg', '.css', '.js', '.srt', '.sub', '.idx', '.nfo', '.txt', '.torrent', '.html', '.php'}

def fetch_dir(url):
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=3) as resp:
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

def clean_title_and_metadata(folder_name, file_name):
    # Year detection
    year_match = re.search(r'\b(19\d{2}|20\d{2})\b', folder_name) or re.search(r'\b(19\d{2}|20\d{2})\b', file_name)
    year = int(year_match.group(1)) if year_match else None
    
    qualities = []
    text_for_qual = f'{folder_name} {file_name}'
    for q in ['2160p', '4K', '1080p', '720p', '480p', 'BluRay', 'BRRip', 'WEBRip', 'WEB-DL', 'HDRip', 'HDTV', 'DVDRip', 'Dual Audio', 'Multi Audio', '3D', 'HEVC', 'x265', 'x264']:
        if re.search(rf'\b{re.escape(q)}\b', text_for_qual, re.I):
            qualities.append(q)
            
    raw = folder_name if folder_name else file_name
    raw = raw.replace('.', ' ').replace('_', ' ')
    
    if year:
        idx = raw.find(str(year))
        if idx > 0:
            title_candidate = raw[:idx].strip(' ()[]-_')
        else:
            title_candidate = raw
    else:
        title_candidate = re.sub(r'(720p|1080p|2160p|4k|bluray|webrip|web-dl|hdrip|dvdrip|hdtv).*', '', raw, flags=re.I)
    
    title_candidate = re.sub(r'\[.*?\]|\(.*?\)', '', title_candidate)
    title_candidate = re.sub(r'\s+', ' ', title_candidate).strip(' -_')
    
    if not title_candidate or len(title_candidate) < 2:
        title_candidate = folder_name if folder_name else file_name
        title_candidate = title_candidate.replace('.', ' ').strip()
        
    return title_candidate, year, ' / '.join(qualities) if qualities else 'Standard'

def create_item(url, is_file, category, server, folder_url=None):
    parsed = urllib.parse.urlparse(url)
    path = urllib.parse.unquote(parsed.path).strip('/')
    parts = path.split('/')
    file_name = parts[-1]
    
    ext = '.' + file_name.split('.')[-1].lower() if '.' in file_name else ''
    if ext in skip_exts:
        return None
        
    folder_name = parts[-2] if len(parts) >= 2 and is_file else parts[-1]
    title, year, quality = clean_title_and_metadata(folder_name, file_name)
    
    f_url = folder_url if folder_url else (url if not is_file else url[:url.rfind('/') + 1])
    
    return {
        'title': title,
        'year': year,
        'quality': quality,
        'category': category,
        'filename': file_name,
        'is_file': is_file,
        'url': url,
        'folder_url': f_url,
        'server': server,
        'path': path
    }

def main():
    print("Starting rapid indexer for 172.16.50.14, 172.16.50.12, and 172.16.50.8...")
    new_items = []
    series_folders = []
    
    # 1. 172.16.50.14 direct sections
    direct_sections_14 = [
        ('http://172.16.50.14/DHAKA-FLIX-14/KOREAN%20TV%20&%20WEB%20Series/', 'Foreign Language Movies / Korean Series', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies%20%281080p%29/', 'Animation Movies (1080p)', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/IMDb%20Top-250%20Movies/', 'IMDb Top-250 Movies', 'DHAKA-FLIX-14'),
    ]
    
    # Nested year-based sections on 172.16.50.14
    nested_roots_14 = [
        ('http://172.16.50.14/DHAKA-FLIX-14/English%20Movies%20%281080p%29/', 'English Movies (1080p)', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/', 'Hindi Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies/', 'Animation Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/South%20Movies/', 'SOUTH INDIAN MOVIES / South Movies', 'DHAKA-FLIX-14'),
        ('http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/Hindi%20Dubbed/', 'SOUTH INDIAN MOVIES / Hindi Dubbed', 'DHAKA-FLIX-14'),
    ]
    
    # 2. 172.16.50.12 TV series roots
    series_roots_12 = [
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%98%85%20%200%20%20%E2%80%94%20%209/', 'TV-WEB-Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A5%20%20A%20%20%E2%80%94%20%20L/', 'TV-WEB-Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A6%20%20M%20%20%E2%80%94%20%20R/', 'TV-WEB-Series', 'DHAKA-FLIX-12'),
        ('http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A6%20%20S%20%20%E2%80%94%20%20Z/', 'TV-WEB-Series', 'DHAKA-FLIX-12'),
    ]
    
    # 3. 172.16.50.8 Games & Software
    games_roots_8 = [
        ('http://172.16.50.8/DHAKA-FLIX-8/PC%20Games/', 'Games / PC Games', 'DHAKA-FLIX-8'),
        ('http://172.16.50.8/DHAKA-FLIX-8/Console%20Games/', 'Games / Console Games', 'DHAKA-FLIX-8'),
        ('http://172.16.50.8/DHAKA-FLIX-8/Software/', 'Software', 'DHAKA-FLIX-8'),
    ]

    # Fetch direct sections
    for url, cat, srv in direct_sections_14:
        print(f"Fetching {cat}...")
        entries = fetch_dir(url)
        print(f"  -> {len(entries)} items")
        for u, is_dir, text in entries:
            item = create_item(u, not is_dir, cat, srv)
            if item:
                new_items.append(item)
            if is_dir:
                series_folders.append((u, cat, srv))
    
    # Fetch nested roots (year subfolders) in parallel
    for root_url, cat, srv in nested_roots_14:
        print(f"Fetching nested {cat}...")
        year_dirs = fetch_dir(root_url)
        print(f"  Found {len(year_dirs)} year subfolders for {cat}")
        
        with ThreadPoolExecutor(max_workers=25) as executor:
            future_to_dir = {executor.submit(fetch_dir, yd[0]): yd for yd in year_dirs if yd[1]}
            for future in as_completed(future_to_dir):
                sub_entries = future.result()
                for u, is_dir, text in sub_entries:
                    item = create_item(u, not is_dir, cat, srv)
                    if item:
                        new_items.append(item)

    # Fetch 172.16.50.12 TV Series in parallel
    for sroot, cat, srv in series_roots_12:
        print(f"Fetching {sroot}...")
        series_dirs = fetch_dir(sroot)
        print(f"  Found {len(series_dirs)} series in group")
        for u, is_dir, text in series_dirs:
            item = create_item(u, not is_dir, cat, srv)
            if item:
                new_items.append(item)
            if is_dir:
                series_folders.append((u, cat, srv))
                
    # Fetch 172.16.50.8 Games in parallel
    for groot, cat, srv in games_roots_8:
        print(f"Fetching {groot}...")
        game_dirs = fetch_dir(groot)
        print(f"  Found {len(game_dirs)} items in group")
        for u, is_dir, text in game_dirs:
            item = create_item(u, not is_dir, cat, srv)
            if item:
                new_items.append(item)

    # Specifically expand Squid Game seasons and episodes so each episode is also searchable!
    squid_targets = [sf for sf in series_folders if 'squid' in sf[0].lower()]
    print(f"Expanding Squid Game episodes ({len(squid_targets)} folders)...")
    for sf, cat, srv in squid_targets:
        seasons = fetch_dir(sf)
        for su, s_isdir, stext in seasons:
            if s_isdir:
                episodes = fetch_dir(su)
                for eu, e_isdir, etext in episodes:
                    item = create_item(eu, not e_isdir, cat, srv, sf)
                    if item:
                        new_items.append(item)

    print(f"Total newly fetched media items: {len(new_items)}")
    
    # Load existing sam_media.json
    existing_file = 'data/sam_media.json'
    existing = []
    seen_urls = set()
    if os.path.exists(existing_file):
        with open(existing_file, 'r', encoding='utf-8') as f:
            existing = json.load(f)
            for it in existing:
                seen_urls.add(it['url'])
                
    print(f"Existing items in {existing_file}: {len(existing)}")
    
    added_count = 0
    for it in new_items:
        if it['url'] not in seen_urls:
            seen_urls.add(it['url'])
            existing.append(it)
            added_count += 1
            
    print(f"Added {added_count} new unique items! Total combined: {len(existing)}")
    
    # Re-assign continuous IDs
    for i, it in enumerate(existing):
        it['id'] = i + 1
        
    with open(existing_file, 'w', encoding='utf-8') as f:
        json.dump(existing, f, indent=2, ensure_ascii=False)
        
    print(f"Successfully updated {existing_file}! Total: {len(existing)}")

if __name__ == '__main__':
    main()
