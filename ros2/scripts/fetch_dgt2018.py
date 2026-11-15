"""Bounded, sequential DGT2018 CC-BY4 WMS cache (four <=3072px GetMaps)."""
from pathlib import Path
import io,json,time,urllib.parse,urllib.request
from PIL import Image
from emberwatch_drone.arganil.geodata import map_to_ll
import numpy as np

out=Path.home()/'px4/geodata/dgt2018';out.mkdir(parents=True,exist_ok=True)
endpoint='https://cartografia.dgterritorio.gov.pt/wms/ortos2018'
lat,lon=map_to_ll(np.array([-1510,1510,-1510,1510]),np.array([-1510,-1510,1510,1510]))
west,east,south,north=float(lon.min()),float(lon.max()),float(lat.min()),float(lat.max())
meta={'endpoint':endpoint,'layer':'Ortos2018-RGB','crs':'EPSG:4326','axis_order':'latitude,longitude (WMS1.3)','bounds_wsen':[west,south,east,north],'pixels':6144,'source_gsd_m':.25,'output_sample_m_approx':3020/6144,'license':'CC BY4.0','license_url':'https://dados.gov.pt/en/datasets/ortofotos-25-cm-portugal-continental-2018/','terms_url':endpoint+'?service=wms&request=getcapabilities','terms':{'Fees':'no conditions apply','AccessConstraints':'None'},'attribution':'Informação geográfica cedida pela Direção-Geral do Território — Ortofotos2018','requests':[]}
used=sum(p.stat().st_size for p in out.glob('*.jpg'))
for row in range(2):
 for col in range(2):
  w=west+(east-west)*col/2;e=west+(east-west)*(col+1)/2
  n=north-(north-south)*row/2;s=north-(north-south)*(row+1)/2
  params={'SERVICE':'WMS','VERSION':'1.3.0','REQUEST':'GetMap','LAYERS':'Ortos2018-RGB','STYLES':'','CRS':'EPSG:4326','BBOX':f'{s:.12f},{w:.12f},{n:.12f},{e:.12f}','WIDTH':3072,'HEIGHT':3072,'FORMAT':'image/jpeg'}
  url=endpoint+'?'+urllib.parse.urlencode(params);p=out/f'{row}_{col}.jpg'
  if not p.exists():
   req=urllib.request.Request(url,headers={'User-Agent':'EmberWatch-academic-simulation/0.1 (bounded DGT2018 sample)'})
   with urllib.request.urlopen(req,timeout=90) as response:
    data=response.read(75_000_001)
   if len(data)>75_000_000 or used+len(data)>300_000_000:raise RuntimeError('Download budget exceeded')
   with Image.open(io.BytesIO(data)) as im:
    im.load();assert im.size==(3072,3072);assert im.convert('RGB').getextrema()!=((255,255),)*3
   p.write_bytes(data);used+=len(data);time.sleep(1)
  meta['requests'].append({'file':p.name,'url':url,'bytes':p.stat().st_size})
  print(p,p.stat().st_size,flush=True)
meta['total_bytes']=used;(out/'metadata.json').write_text(json.dumps(meta,indent=2));print('DOWNLOAD_BYTES',used)
