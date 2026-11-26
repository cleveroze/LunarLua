"""Build static preview variants of the UI for screenshot review."""
import json
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
html = open(os.path.join(ROOT, "web", "index.html"), encoding="utf-8").read()

sample = open(os.path.join(ROOT, "web", "assets", "sample.lua"), encoding="utf-8").read()
recovered = open(os.path.join(ROOT, "build", "out-sample.lua"), encoding="utf-8").read()


def demo(input_text: str, output_text: str, status: str, detected: str, ms: int) -> str:
    return f"""
<script>
window.addEventListener('load', () => setTimeout(() => {{
  const ta = document.getElementById('deobIn');
  ta.value = {json.dumps(input_text)};
  ta.dispatchEvent(new Event('input'));
  document.getElementById('deobOut').innerHTML = highlightLua({json.dumps(output_text)});
  document.getElementById('deobGutterOut').innerHTML = gutterFor({json.dumps(output_text)});
  document.getElementById('deobOutMeta').textContent = '18 lines · 1.1 KB';
  document.getElementById('deobStatus').textContent = {json.dumps(status)};
  document.getElementById('deobPulse').className = 'status-pulse ok';
  document.getElementById('deobDetected').textContent = {json.dumps(detected)};
  document.getElementById('meterValue').textContent = '{ms} ms';
  document.getElementById('meterLabel').textContent = 'last run';
  document.getElementById('meterFill').style.width = '34%';
}}, 80));
</script>
"""


def write(name: str, theme: str, view: str) -> None:
    doc = html.replace('<html lang="en" data-theme="dark">', f'<html lang="en" data-theme="{theme}">')
    doc = doc.replace("<head>", "<head><script>try{localStorage.setItem('ll.theme','" + theme + "')}catch(e){}</script>")
    if view == "obf":
        doc = doc.replace('class="view is-active" id="view-deob"', 'class="view" id="view-deob"')
        doc = doc.replace('class="view" id="view-obf"', 'class="view is-active" id="view-obf"')
        doc = doc.replace('class="nav-item is-active" data-tab="deob"', 'class="nav-item" data-tab="deob"')
        doc = doc.replace('class="nav-item" data-tab="obf"', 'class="nav-item is-active" data-tab="obf"')
        doc = doc.replace("</body>", demo(sample, recovered, "Done in 14 ms", "Obfuscated", 14) + "</body>")
    else:
        doc = doc.replace("</body>", demo(sample, recovered, "Done in 12 ms", "WeAreDevs VM pack", 12) + "</body>")
    open(os.path.join(ROOT, "web", name), "w", encoding="utf-8", newline="\n").write(doc)
    print("wrote", name)


write("preview-dark.html", "dark", "deob")
write("preview-light.html", "light", "deob")
write("preview-obf.html", "dark", "obf")
