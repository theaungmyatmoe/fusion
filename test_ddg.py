import sys
import json
import urllib.parse
import urllib.request
import re

try:
    import requests
    from bs4 import BeautifulSoup
    headers = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"}
    def search_requests(query):
        url = "https://lite.duckduckgo.com/lite/"
        resp = requests.post(url, headers=headers, data={"q": query}, timeout=10)
        print(f"requests status: {resp.status_code}")
        if resp.status_code == 200:
            soup = BeautifulSoup(resp.text, 'html.parser')
            results = []
            links = soup.find_all("a", class_="result-link")
            for link in links:
                href = link.get("href", "")
                title = link.text.strip()
                parent_tr = link.find_parent("tr")
                snippet = ""
                if parent_tr:
                    next_tr = parent_tr.find_next_sibling("tr")
                    if next_tr and next_tr.select_one(".result-snippet"):
                        snippet = next_tr.select_one(".result-snippet").text.strip()
                results.append({"title": title, "url": href, "snippet": snippet})
            if results:
                return results
        url = f"https://html.duckduckgo.com/html/?q={urllib.parse.quote(query)}"
        resp = requests.get(url, headers=headers, timeout=10)
        print(f"requests fallback status: {resp.status_code}")
        if resp.status_code == 200:
            soup = BeautifulSoup(resp.text, 'html.parser')
            results = []
            for r in soup.select(".result"):
                title_elem = r.select_one(".result__a")
                if not title_elem:
                    continue
                href = title_elem.get("href", "")
                title = title_elem.text.strip()
                snippet_elem = r.select_one(".result__snippet")
                snippet = snippet_elem.text.strip() if snippet_elem else ""
                results.append({"title": title, "url": href, "snippet": snippet})
            return results
        return None
except Exception as e:
    print(f"requests import/setup failed: {e}")
    search_requests = None

def search_urllib(query):
    url = "https://lite.duckduckgo.com/lite/"
    data = urllib.parse.urlencode({"q": query}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"})
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            html = response.read().decode("utf-8")
            results = []
            tr_blocks = re.findall(r'<tr.*?>(.*?)</tr>', html, re.DOTALL)
            current_link = None
            for tr in tr_blocks:
                link_match = re.search(r'<a[^>]+class="result-link"[^>]+href="([^"]+)"[^>]*>(.*?)</a>', tr, re.DOTALL)
                if link_match:
                    url = link_match.group(1)
                    title = re.sub(r'<[^>]+>', '', link_match.group(2)).strip()
                    current_link = {"title": title, "url": url, "snippet": ""}
                    continue
                snippet_match = re.search(r'<td[^>]+class="result-snippet"[^>]*>(.*?)</td>', tr, re.DOTALL)
                if snippet_match and current_link:
                    snippet = re.sub(r'<[^>]+>', '', snippet_match.group(1)).strip()
                    snippet = re.sub(r'\s+', ' ', snippet)
                    current_link["snippet"] = snippet
                    results.append(current_link)
                    current_link = None
            return results
    except Exception as e:
        print(f"urllib failed: {e}")
        return []

def main():
    query = "Aung Myat Moe"
    results = None
    if search_requests:
        try:
            results = search_requests(query)
        except Exception as e:
            print(f"search_requests error: {e}")
            pass
    if not results:
        results = search_urllib(query)
    print(json.dumps({"results": results}))

if __name__ == "__main__":
    main()
