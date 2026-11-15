"use client";
import {useCallback,useEffect,useRef,useState} from "react";
import {lisbon,compassPoint} from "@/lib/replay";
import s from "./ops.module.css";
type Weather={name:string;status:string;current:Record<string,number|string|null>|null;forecast:{time:string;wind_kmh:number|null;gust_kmh:number|null;direction_deg:number|null}[]};
type Source={name:string;url:string;category:string;status:string;captured_at:string|null;request_id:string|null;credits:number|null;error_code:string|null;headline?:string;items:({title?:string;url:string;quote?:string;published_at?:string|null}|Weather)[]};
type Data={mode:"live";checked_at:string;status:string;sources:Source[];context_budget?:{actual:number;limit:number;outcome_unknown:boolean};note:string};
const number=(value:unknown,suffix="")=>typeof value==="number"&&Number.isFinite(value)?`${Math.round(value*10)/10}${suffix}`:"Unknown";
function WindChart({hours}:{hours:Weather["forecast"]}){
  const finite=hours.flatMap(h=>[h.wind_kmh,h.gust_kmh]).filter((v):v is number=>typeof v==="number"&&Number.isFinite(v));
  if(!finite.length)return <span>Forecast unavailable</span>;
  const max=Math.max(20,...finite);const line=(key:"wind_kmh"|"gust_kmh")=>hours.map((h,i)=>typeof h[key]==="number"?`${10+i*280/Math.max(1,hours.length-1)},${60-Number(h[key])/max*48}`:null);
  return <svg viewBox="0 0 300 76" className={s.windChart} role="img" aria-label="Next twelve forecast hours: blue wind speed, amber gusts in kilometres per hour">
    <path d="M10 60H290" stroke="#40535f" fill="none"/>
    {(["wind_kmh","gust_kmh"]as const).flatMap(key=>{const segments:string[][]=[[]];for(const p of line(key)){if(p)segments[segments.length-1].push(p);else segments.push([]);}return segments.filter(p=>p.length).map((points,i)=><polyline key={key+i} points={points.join(" ")} fill="none" stroke={key==="wind_kmh"?"#7ed5ef":"#e8b166"} strokeWidth="2"/>);})}
    <text x="10" y="74" fill="#a5afb7" fontSize="9">Next 12 forecast hours</text><text x="290" y="10" textAnchor="end" fill="#a5afb7" fontSize="9">{Math.ceil(max)} km/h</text>
  </svg>;
}
export default function NationalData(){
  const [data,setData]=useState<Data|null>(null),[error,setError]=useState(false),[loading,setLoading]=useState(true);
  const controller=useRef<AbortController|null>(null);
  const refresh=useCallback(async()=>{
    if(controller.current)return;const abort=new AbortController();controller.current=abort;setLoading(true);
    try{const r=await fetch("/api/live/portugal",{cache:"no-store",signal:abort.signal});if(!r.ok)throw Error();const v=await r.json();if(v.mode!=="live"||!Array.isArray(v.sources))throw Error();setData(v);setError(false);}
    catch{if(!abort.signal.aborted)setError(true);}finally{if(controller.current===abort){controller.current=null;setLoading(false);}}
  },[]);
  useEffect(()=>{const first=setTimeout(()=>void refresh(),0);const timer=setInterval(()=>{if(document.visibilityState==="visible")void refresh();},300_000);return()=>{clearTimeout(first);clearInterval(timer);controller.current?.abort();controller.current=null;};},[refresh]);
  const weather=data?.sources.find(s=>s.category==="weather");
  return <section className={s.livePanel} aria-label="Portugal news, weather and wind">
    <div className={s.livePanelHead}><div><h2>Portugal news, weather &amp; wind</h2><span className={error?s.connectionBad:s.connectionGood}>{loading?"Collecting national sources…":error?"Connection unavailable":data?.status==="disabled"?"Collection disabled":`${data?.sources.length??0} source operations · ${data?.status??"waiting"}`}</span></div><button className={s.refreshData} disabled={loading} onClick={()=>void refresh()}>{loading?"Collecting…":"Refresh sources"}</button></div>
    <p className={s.muted}>Background collection runs even without a wildfire. Dashboard checks every 5 minutes; source captures are cached for 15 minutes. News is discovered through Context.dev and public pages are scraped.</p>
    {error&&<p className={s.liveWarning}>National sources are unavailable. {data?"Last received results below may be stale.":"No saved demo data has been substituted."}</p>}
    {data&&<>
      <p className={s.muted}>Collected {lisbon(data.checked_at,"en",true)} · Lisbon time{data.context_budget&&` · Context.dev: ${data.context_budget.actual} / ${data.context_budget.limit} credits in the current hour`}</p>
      {data.context_budget?.outcome_unknown&&<p className={s.liveWarning}>A billed request has an unknown outcome. Further Context calls are paused in this budget.</p>}
      <h3>Regional weather and wind watch</h3><p className={s.muted}>Nine mainland coverage points. Model conditions, not direct station measurements. Wind and gusts in km/h; hourly forecasts use UTC.</p>
      <div className={s.weatherGrid}>{(weather?.items??[]).map(item=>{const row=item as Weather;const c=row.current;return <article className={s.weatherTile} key={row.name}><h4>{row.name}</h4><span>{c?.time?`${String(c.time)} UTC`:"Time unavailable"}</span>
        <strong>{number(c?.temperature_2m," °C")}</strong><dl><div><dt>Wind</dt><dd>{number(c?.wind_speed_10m," km/h")} {typeof c?.wind_direction_10m==="number"?compassPoint(c.wind_direction_10m,"en"):""}</dd></div><div><dt>Gusts</dt><dd>{number(c?.wind_gusts_10m," km/h")}</dd></div><div><dt>Humidity</dt><dd>{number(c?.relative_humidity_2m," %")}</dd></div><div><dt>Rain</dt><dd>{number(c?.precipitation," mm")}</dd></div></dl><WindChart hours={row.forecast??[]}/></article>;})}</div>
      {weather&&<p className={s.muted}><a href={weather.url} target="_blank" rel="noreferrer">Weather by Open-Meteo · CC BY 4.0 ↗</a> · {weather.status}</p>}
      <h3>News &amp; public weather pages</h3><div className={s.liveSources}>{data.sources.filter(src=>src.category!=="weather").map((src,i)=><article className={s.liveSource} key={src.url+i}>
        <div className={s.liveSourceHead}><a href={src.url} target="_blank" rel="noreferrer">{src.headline??src.name} ↗</a><strong className={src.status==="ok"?s.connectionGood:s.connectionBad}>{src.status}</strong></div>
        <p>{src.category.replaceAll("_"," ")} · {src.captured_at?lisbon(src.captured_at,"en",true):"Not captured"} · {src.credits===null?"credits unknown":`${src.credits} credits`}</p>
        {src.error_code&&<p className={s.liveWarning}>{src.error_code}</p>}
        <ul className={s.newsItems}>{src.items.map((item,i)=>{const article=item as {title?:string;url:string;quote?:string};return <li key={i}>{article.title?<a href={article.url} target="_blank" rel="noreferrer">{article.title}</a>:<blockquote>{article.quote}</blockquote>}</li>;})}</ul>
        {src.request_id&&<code className={s.basis}>Request: {src.request_id}</code>}
      </article>)}</div><p className={s.muted}>{data.note}</p>
    </>}
  </section>;
}
