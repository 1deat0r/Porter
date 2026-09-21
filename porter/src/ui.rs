pub const PAGE: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>porter settings</title>
<style>
body{font-family:system-ui,sans-serif;max-width:860px;margin:2em auto;padding:0 1em;color:#222}
.tabs{display:flex;gap:.5em;margin:1em 0}
.tabs button{padding:.5em 1em;border:1px solid #ccc;background:#f4f4f4;cursor:pointer;border-radius:6px}
.tabs button.on{background:#222;color:#fff}
.tab{display:none}.tab.on{display:block}
table{border-collapse:collapse;width:100%}
th,td{border-bottom:1px solid #eee;padding:.45em;text-align:left;vertical-align:middle}
input[type=password],input[type=text]{width:220px;padding:.35em;border:1px solid #ccc;border-radius:5px}
.dot{display:inline-block;width:.7em;height:.7em;border-radius:50%;background:#c33;margin-right:.4em}
.dot.y{background:#2a2}
.grp td{background:#fafafa;font-weight:600}
.mut{color:#666;font-size:.85em}
button.sv{padding:.3em .7em;border-radius:5px;border:1px solid #ccc;background:#fff;cursor:pointer}
</style></head><body>
<h1>porter settings</h1>
<div class="tabs">
<button data-t="general" class="on">General</button>
<button data-t="connectors">Connectors</button>
<button data-t="agents">Agents</button>
</div>
<div id="general" class="tab on">
<p>Status: <span id="health">…</span></p>
<p class="mut">Vault file: <span id="vpath"></span> (0600). Keys never leave this host; the UI only shows presence, never values.</p>
<p>Default port <b>8819</b>. systemd user unit <span class="mut">porter.service</span>.</p>
</div>
<div id="connectors" class="tab">
<p><label>Scope <select id="scope">
<option value="global">global (shared)</option>
<option value="codex">codex</option>
<option value="hermes">hermes</option>
<option value="omp">omp</option>
</select></label> <span class="mut">per-agent keys override global; Jev falls back to TYPESAFE_API_KEY.</span></p>
<table><thead><tr><th>Connector</th><th>API key</th><th>Endpoint</th></tr></thead><tbody id="rows"></tbody></table>
</div>
<div id="agents" class="tab">
<p class="mut">Each agent uses its own Jev key (<span class="mut">codex/typesafe</span> etc.), else the shared one.</p>
<table><thead><tr><th>Agent</th><th>Jev key</th></tr></thead><tbody id="arows"></tbody></table>
</div>
<script>
const GROUPS=[["Jev",["typesafe"]],["LLMs",["meta","deepseek","openai","anthropic","openrouter","gemini"]],["Apps",["notion","supabase","cloudflare","minara"]]];
const scopeEl=document.getElementById('scope');
document.querySelectorAll('.tabs button').forEach(b=>b.onclick=()=>{
document.querySelectorAll('.tabs button').forEach(x=>x.classList.remove('on'));
document.querySelectorAll('.tab').forEach(x=>x.classList.remove('on'));
b.classList.add('on');document.getElementById(b.dataset.t).classList.add('on');});
async function health(){try{const r=await fetch('/health');const j=await r.json();
document.getElementById('health').textContent=j.ok?'running':'down';}catch(e){document.getElementById('health').textContent='down';}}
async function load(){const s=scopeEl.value;const r=await fetch('/v1/vault/status?scope='+s);const j=await r.json();
document.getElementById('vpath').textContent=j.vault||'';
const tb=document.getElementById('rows');tb.innerHTML='';
for(const [g,items] of GROUPS){const tr=document.createElement('tr');tr.className='grp';
tr.innerHTML='<td colspan="3">'+g+'</td>';tb.appendChild(tr);
for(const p of items){const st=(j.providers||[]).find(x=>x.provider===p)||{};
const tr2=document.createElement('tr');
tr2.innerHTML='<td><span class="dot '+(st.has_key?'y':'')+'"></span>'+p+(st.has_key?' <span class="mut">set</span>':' <span class="mut">missing</span>')+'</td>'
+'<td><input type="password" id="k_'+p+'" placeholder="'+(st.has_key?'•••••• (saved)':'paste key')+'" autocomplete="off"> <button class="sv" data-k="'+p+'">Save</button></td>'
+'<td><input type="text" id="e_'+p+'" value="'+(st.endpoint||'').replace(/"/g,'&quot;')+'"> <button class="sv" data-e="'+p+'">Save</button> '+(st.custom?'<button class="sv" data-c="'+p+'">Reset</button>':'')+'</td>';
tb.appendChild(tr2);}}
tb.querySelectorAll('[data-k]').forEach(b=>b.onclick=()=>saveKey(b.dataset.k));
tb.querySelectorAll('[data-e]').forEach(b=>b.onclick=()=>saveEp(b.dataset.e));
tb.querySelectorAll('[data-c]').forEach(b=>b.onclick=()=>clearEp(b.dataset.c));
const ab=document.getElementById('arows');ab.innerHTML='';
for(const a of ['codex','hermes','omp']){const r2=await fetch('/v1/vault/status?scope='+a);const j2=await r2.json();
const t=(j2.providers||[]).find(x=>x.provider==='typesafe')||{};
const tr=document.createElement('tr');tr.innerHTML='<td>'+a+'</td><td><span class="dot '+(t.has_key?'y':'')+'"></span>'+(t.has_key?'set':'missing')+'</td>';ab.appendChild(tr);}}
async function saveKey(p){const v=document.getElementById('k_'+p).value;if(!v){alert('paste a key first');return;}
const r=await fetch('/v1/vault/set',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({scope:scopeEl.value,provider:p,key:v})});
const j=await r.json();if(j.ok){document.getElementById('k_'+p).value='';load();}else alert(j.error||'save failed');}
async function saveEp(p){const v=document.getElementById('e_'+p).value;
const r=await fetch('/v1/vault/set',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({scope:scopeEl.value,provider:p,endpoint:v})});
const j=await r.json();if(j.ok)load();else alert(j.error||'save failed');}
async function clearEp(p){const r=await fetch('/v1/vault/set',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({scope:scopeEl.value,provider:p,endpoint:''})});
const j=await r.json();if(j.ok)load();else alert(j.error||'reset failed');}
scopeEl.onchange=load;health();load();
</script></body></html>"##;
