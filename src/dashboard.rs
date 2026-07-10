use crate::{config::AppConfig, model::ExtractedRecord};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};
use tokio::fs;

#[derive(Debug, Serialize)]
pub struct DashboardBuild {
    pub output_path: String,
    pub records: usize,
    pub rules: usize,
    pub sources: usize,
}

#[derive(Debug, Serialize)]
struct DashboardPayload {
    generated_at: String,
    records: Vec<ExtractedRecord>,
    summary: Option<Value>,
}

pub async fn generate(config: &AppConfig, output: &Path) -> Result<DashboardBuild> {
    let raw = fs::read_to_string(&config.storage.jsonl_path)
        .await
        .with_context(|| {
            format!(
                "cannot read extracted records from {}",
                config.storage.jsonl_path
            )
        })?;

    let mut records = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<ExtractedRecord>(line).with_context(|| {
            format!(
                "invalid JSONL record at line {} in {}",
                index + 1,
                config.storage.jsonl_path
            )
        })?;
        records.push(record);
    }

    if records.is_empty() {
        bail!(
            "{} contains no extracted records; run the crawler first",
            config.storage.jsonl_path
        );
    }

    let summary =
        match fs::read_to_string(&config.storage.summary_path).await {
            Ok(raw) => Some(serde_json::from_str::<Value>(&raw).with_context(|| {
                format!("invalid summary JSON in {}", config.storage.summary_path)
            })?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot read {}", config.storage.summary_path));
            }
        };

    let rules = records
        .iter()
        .map(|record| record.rule.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let sources = records
        .iter()
        .map(|record| record.source_url.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    let payload = DashboardPayload {
        generated_at: chrono::Utc::now().to_rfc3339(),
        records,
        summary,
    };
    let payload_json = safe_script_json(&payload)?;
    let html = DASHBOARD_HTML.replace("__DASHBOARD_PAYLOAD__", &payload_json);

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(output, html)
        .await
        .with_context(|| format!("cannot write dashboard {}", output.display()))?;

    Ok(DashboardBuild {
        output_path: output.display().to_string(),
        records: payload.records.len(),
        rules,
        sources,
    })
}

fn safe_script_json<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(json
        .replace("</", "<\\/")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

const DASHBOARD_HTML: &str = r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rama Controlled Extractor · Resultados</title>
<style>
:root{color-scheme:light;--bg:#f4f7fb;--surface:#fff;--surface2:#eef3f9;--text:#172033;--muted:#667085;--line:#dbe3ee;--accent:#2563eb;--accent2:#0f766e;--danger:#b42318;--shadow:0 16px 40px rgba(15,23,42,.09);font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text)}button,input,select{font:inherit}a{color:inherit}.shell{max-width:1500px;margin:auto;padding:24px}.hero{background:linear-gradient(135deg,#102a56,#155e75);color:#fff;border-radius:24px;padding:28px;box-shadow:var(--shadow)}.hero-row{display:flex;justify-content:space-between;gap:20px;align-items:flex-start}.eyebrow{font-size:.76rem;font-weight:800;letter-spacing:.12em;text-transform:uppercase;opacity:.75}.hero h1{font-size:clamp(1.8rem,4vw,3rem);line-height:1.05;margin:.4rem 0}.hero p{max-width:850px;margin:.4rem 0;color:#d8e7f5}.hero code{background:#ffffff18;border:1px solid #ffffff2b;padding:.2rem .45rem;border-radius:8px}.status{display:inline-flex;align-items:center;gap:.5rem;background:#ffffff18;border:1px solid #ffffff2e;padding:.55rem .8rem;border-radius:999px;white-space:nowrap}.dot{width:9px;height:9px;border-radius:50%;background:#5eead4;box-shadow:0 0 0 5px #5eead426}.metrics{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:12px;margin:18px 0}.metric{background:var(--surface);border:1px solid var(--line);border-radius:18px;padding:18px;box-shadow:0 8px 22px rgba(15,23,42,.045)}.metric span{display:block;font-size:.78rem;color:var(--muted);text-transform:uppercase;letter-spacing:.07em;font-weight:800}.metric strong{display:block;font-size:1.75rem;margin-top:5px}.toolbar{position:sticky;top:0;z-index:10;margin:18px 0;background:rgba(244,247,251,.92);backdrop-filter:blur(14px);padding:12px 0}.controls{display:grid;grid-template-columns:minmax(240px,2fr) minmax(180px,1fr) minmax(180px,1fr) auto auto;gap:10px;background:var(--surface);border:1px solid var(--line);padding:12px;border-radius:18px;box-shadow:0 8px 24px rgba(15,23,42,.06)}.control{border:1px solid var(--line);background:#fff;color:var(--text);border-radius:12px;padding:.78rem .9rem;min-width:0}.button{border:0;border-radius:12px;padding:.78rem 1rem;background:var(--accent);color:#fff;font-weight:800;cursor:pointer}.button.secondary{background:var(--surface2);color:var(--text);border:1px solid var(--line)}.result-line{display:flex;justify-content:space-between;align-items:center;margin:12px 3px;color:var(--muted);font-size:.9rem}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px}.card{background:var(--surface);border:1px solid var(--line);border-radius:20px;overflow:hidden;box-shadow:0 10px 24px rgba(15,23,42,.05);display:flex;flex-direction:column;min-height:300px}.media{aspect-ratio:16/8.5;background:linear-gradient(135deg,#dbeafe,#ccfbf1);overflow:hidden}.media img{width:100%;height:100%;object-fit:cover;display:block}.placeholder{height:100%;display:grid;place-items:center;font-size:2rem;font-weight:900;color:#155e75}.content{padding:18px;display:flex;flex-direction:column;gap:10px;flex:1}.badges{display:flex;flex-wrap:wrap;gap:7px}.badge{font-size:.72rem;font-weight:800;padding:.35rem .55rem;border-radius:999px;background:#dbeafe;color:#1d4ed8}.badge.rule-page{background:#ccfbf1;color:#0f766e}.card h2{font-size:1.08rem;line-height:1.35;margin:0}.summary{color:var(--muted);line-height:1.55;font-size:.92rem;display:-webkit-box;-webkit-line-clamp:4;-webkit-box-orient:vertical;overflow:hidden}.meta{font-size:.78rem;color:var(--muted);margin-top:auto}.actions{display:flex;gap:8px;flex-wrap:wrap}.link,.detail{font-size:.82rem;font-weight:800;text-decoration:none;border:1px solid var(--line);border-radius:10px;padding:.55rem .7rem;background:#fff;cursor:pointer}.link{color:var(--accent)}.empty{display:none;text-align:center;background:var(--surface);border:1px dashed var(--line);padding:50px;border-radius:20px;color:var(--muted)}dialog{width:min(900px,calc(100vw - 28px));max-height:85vh;border:0;border-radius:22px;padding:0;box-shadow:0 30px 100px rgba(15,23,42,.35)}dialog::backdrop{background:rgba(15,23,42,.62);backdrop-filter:blur(4px)}.dialog-head{position:sticky;top:0;background:#fff;border-bottom:1px solid var(--line);padding:16px 20px;display:flex;justify-content:space-between;align-items:center}.dialog-body{padding:20px;overflow:auto}.dialog-body pre{white-space:pre-wrap;word-break:break-word;background:#0f172a;color:#dbeafe;padding:18px;border-radius:14px;font-size:.82rem;line-height:1.55}.close{border:0;background:var(--surface2);border-radius:10px;padding:.55rem .75rem;cursor:pointer}.footer{text-align:center;color:var(--muted);font-size:.8rem;padding:28px}
@media(max-width:1100px){.metrics{grid-template-columns:repeat(3,1fr)}.grid{grid-template-columns:repeat(2,1fr)}.controls{grid-template-columns:1fr 1fr}.controls .search{grid-column:1/-1}}
@media(max-width:700px){.shell{padding:12px}.hero{padding:20px;border-radius:18px}.hero-row{display:block}.status{margin-top:14px}.metrics{grid-template-columns:1fr 1fr}.grid{grid-template-columns:1fr}.controls{grid-template-columns:1fr}.controls .search{grid-column:auto}.toolbar{position:static}}
</style>
</head>
<body>
<div class="shell">
  <section class="hero">
    <div class="hero-row">
      <div><div class="eyebrow">Rama Controlled Extractor</div><h1>Resultados de extracción</h1><p>Visor local y autónomo. Los datos permanecen en este archivo HTML y no se envían a servicios externos.</p></div>
      <div class="status"><span class="dot"></span> Datos cargados</div>
    </div>
  </section>

  <section class="metrics" aria-label="Resumen">
    <div class="metric"><span>Registros</span><strong id="mRecords">0</strong></div>
    <div class="metric"><span>Páginas</span><strong id="mPages">—</strong></div>
    <div class="metric"><span>Reglas</span><strong id="mRules">0</strong></div>
    <div class="metric"><span>Fuentes</span><strong id="mSources">0</strong></div>
    <div class="metric"><span>Descargado</span><strong id="mBytes">—</strong></div>
  </section>

  <div class="toolbar">
    <div class="controls">
      <input id="search" class="control search" type="search" placeholder="Buscar por título, resumen, fecha, URL…" autocomplete="off">
      <select id="rule" class="control"><option value="">Todas las reglas</option></select>
      <select id="source" class="control"><option value="">Todas las fuentes</option></select>
      <button id="reset" class="button secondary" type="button">Limpiar</button>
      <button id="export" class="button" type="button">Exportar JSON</button>
    </div>
  </div>

  <div class="result-line"><span id="count">0 resultados</span><span id="generated"></span></div>
  <main id="grid" class="grid"></main>
  <section id="empty" class="empty"><h2>Sin resultados</h2><p>Cambia los filtros o la búsqueda.</p></section>
  <footer class="footer">Generado localmente por Rama Controlled Extractor.</footer>
</div>

<dialog id="details">
  <div class="dialog-head"><strong id="detailTitle">Detalle del registro</strong><button id="close" class="close" type="button">Cerrar</button></div>
  <div class="dialog-body"><pre id="detailJson"></pre></div>
</dialog>

<script id="payload" type="application/json">__DASHBOARD_PAYLOAD__</script>
<script>
'use strict';
const payload=JSON.parse(document.getElementById('payload').textContent);
const records=Array.isArray(payload.records)?payload.records:[];
const summary=payload.summary||{};
const $=id=>document.getElementById(id);
const search=$('search'),rule=$('rule'),source=$('source'),grid=$('grid'),empty=$('empty'),details=$('details');
const collator=new Intl.Collator('ca',{sensitivity:'base',numeric:true});
const unique=(items)=>[...new Set(items.filter(Boolean))].sort(collator.compare);
const text=(v)=>v==null?'':Array.isArray(v)?v.join(' '):typeof v==='object'?JSON.stringify(v):String(v);
const safeUrl=(value)=>{try{const u=new URL(String(value));return ['http:','https:'].includes(u.protocol)?u.href:null}catch{return null}};
const titleOf=(r)=>text(r.data.title||r.data.page_title||r.data.name||r.rule||'Registro sin título');
const summaryOf=(r)=>text(r.data.summary||r.data.first_paragraph||r.data.body||r.data.headings||'');
const imageOf=(r)=>safeUrl(r.data.image_url||(Array.isArray(r.data.images)?r.data.images[0]:''));
const linkOf=(r)=>safeUrl(r.data.url||r.data._source_url||r.source_url);
const haystack=(r)=>[r.rule,r.source_url,r.extracted_at,...Object.values(r.data||{}).map(text)].join(' ').toLocaleLowerCase('ca');

for(const value of unique(records.map(r=>r.rule))){const o=document.createElement('option');o.value=value;o.textContent=value;rule.append(o)}
for(const value of unique(records.map(r=>r.source_url))){const o=document.createElement('option');o.value=value;o.textContent=value;source.append(o)}

$('mRecords').textContent=records.length.toLocaleString('es-ES');
$('mPages').textContent=summary.pages_fetched==null?'—':Number(summary.pages_fetched).toLocaleString('es-ES');
$('mRules').textContent=unique(records.map(r=>r.rule)).length;
$('mSources').textContent=unique(records.map(r=>r.source_url)).length;
$('mBytes').textContent=summary.bytes_downloaded==null?'—':formatBytes(Number(summary.bytes_downloaded));
$('generated').textContent='Generado: '+new Date(payload.generated_at).toLocaleString('es-ES');

function formatBytes(bytes){if(!Number.isFinite(bytes)||bytes<1)return '0 B';const units=['B','KB','MB','GB'];const i=Math.min(Math.floor(Math.log(bytes)/Math.log(1024)),units.length-1);return (bytes/1024**i).toLocaleString('es-ES',{maximumFractionDigits:1})+' '+units[i]}
function filtered(){const q=search.value.trim().toLocaleLowerCase('ca');return records.filter(r=>(!rule.value||r.rule===rule.value)&&(!source.value||r.source_url===source.value)&&(!q||haystack(r).includes(q)))}
function node(tag,className,content){const el=document.createElement(tag);if(className)el.className=className;if(content!=null)el.textContent=content;return el}
function render(){const items=filtered();grid.replaceChildren();$('count').textContent=items.length.toLocaleString('es-ES')+' resultado'+(items.length===1?'':'s');empty.style.display=items.length?'none':'block';
  for(const record of items){
    const card=node('article','card');const media=node('div','media');const image=imageOf(record);
    if(image){const img=document.createElement('img');img.src=image;img.alt='';img.loading='lazy';img.referrerPolicy='no-referrer';img.onerror=()=>{media.replaceChildren(node('div','placeholder','R'))};media.append(img)}else{media.append(node('div','placeholder','R'))}
    const body=node('div','content');const badges=node('div','badges');const badge=node('span','badge '+(record.rule.includes('page')?'rule-page':''),record.rule);badges.append(badge);
    if(record.data.date)badges.append(node('span','badge',text(record.data.date)));
    body.append(badges,node('h2','',titleOf(record)));
    const description=summaryOf(record);if(description)body.append(node('div','summary',description));
    body.append(node('div','meta',new URL(record.source_url).hostname+' · '+new Date(record.extracted_at).toLocaleString('es-ES')));
    const actions=node('div','actions');const link=linkOf(record);if(link){const a=node('a','link','Abrir fuente');a.href=link;a.target='_blank';a.rel='noopener noreferrer';actions.append(a)}
    const detail=node('button','detail','Ver JSON');detail.type='button';detail.addEventListener('click',()=>showDetails(record));actions.append(detail);body.append(actions);card.append(media,body);grid.append(card)
  }
}
function showDetails(record){$('detailTitle').textContent=titleOf(record);$('detailJson').textContent=JSON.stringify(record,null,2);details.showModal()}
$('close').addEventListener('click',()=>details.close());details.addEventListener('click',e=>{if(e.target===details)details.close()});
for(const el of [search,rule,source])el.addEventListener(el===search?'input':'change',render);
$('reset').addEventListener('click',()=>{search.value='';rule.value='';source.value='';render();search.focus()});
$('export').addEventListener('click',()=>{const blob=new Blob([JSON.stringify(filtered(),null,2)],{type:'application/json'});const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='rama-resultados-filtrados.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000)});
render();
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::safe_script_json;

    #[test]
    fn script_json_escapes_closing_tags() {
        let encoded = safe_script_json(&"</script>").unwrap_or_default();
        assert_eq!(encoded, "\"<\\/script>\"");
    }
}
