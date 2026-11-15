"""Named-control crops for manual DGT2018/OSM registration assessment."""
from pathlib import Path
import json
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from emberwatch_drone.arganil.geodata import DgtOrtho,load_osm,to_map
root=Path(__file__).resolve().parents[1];data=Path.home()/'px4/geodata'
sat=DgtOrtho(str(data/'dgt2018'));osm=load_osm(str(data/'osm_arganil.json'))
controls=[('Visconde Sanches de Frias /\nDr Alberto Moura Pinto',40.2187321,-8.053269),('Dr Fernando do Vale /\nMaestro Alves Coelho',40.2162977,-8.0535391),('Ramiro Castanheira Jorge /\nComendador Saúl Brandão',40.2204414,-8.054593)]
fig,axs=plt.subplots(2,3,figsize=(18,11),layout='constrained');records=[]
for i,(name,lat,lon) in enumerate(controls):
 x,y=to_map(lat,lon);g=np.arange(-45,45,.25)+.125;E,N=np.meshgrid(g+x,g[::-1]+y);rgb=sat.rgb_map(E,N).astype('uint8')
 for row in range(2):
  ax=axs[row,i];ax.imshow(rgb,extent=(-45,45,-45,45),origin='upper');ax.plot(0,0,'r+',ms=15);ax.set(xlim=(-45,45),ylim=(-45,45),xlabel='East of OSM node (m)',ylabel='North of OSM node (m)');ax.grid(alpha=.25);ax.set_title(name+('\nOSM vectors' if row else '\nDGT2018 image / OSM node cross'))
  if row:
   for k,line in osm.lines:
    a=np.asarray(line)-[x,y];ax.plot(a[:,0],a[:,1],color='cyan' if k=='road' else 'blue',lw=1)
   for b in osm.buildings:
    a=np.asarray(b.poly+[b.poly[0]])-[x,y];ax.plot(a[:,0],a[:,1],color='yellow',lw=.7)
 delta=[(-1,0),(0,0),(-1,1)][i]
 records.append({'name':name.replace('\n',' '),'osm_lat_lon':[lat,lon],'enu':[x,y], 'manual_image_minus_osm_enu_m':delta,'apparent_residual_m':round((delta[0]**2+delta[1]**2)**.5,1),'uncertainty_m':3,'method':'manual visual centreline-junction estimate from r2-w1-registration.png; not surveyed geolocation validation'})
fig.suptitle('DGT Ortofotos2018 CC BY4.0 — Informação geográfica cedida pela Direção-Geral do Território\n© OpenStreetMap contributors ODbL; axes in metres, red cross is the named OSM junction; image-grid sample .25m, raw WMS ~.492m')
fig.savefig(root/'collab/shots/r2-w1-registration.png',dpi=140);plt.close(fig)
(root/'generated/static_scene/r2-controls.json').write_text(json.dumps(records,indent=2));print(json.dumps(records,indent=2))
# Whole core context makes the river/road/footprint relation visible at once.
g=np.arange(-1500,1500,2.)+1;E,N=np.meshgrid(g,g[::-1]);rgb=sat.rgb_map(E,N).astype('uint8')
fig,axs=plt.subplots(1,2,figsize=(16,8),layout='constrained')
for ax in axs:ax.imshow(rgb,extent=(-1500,1500,-1500,1500),origin='upper');ax.set(xlim=(-1500,1500),ylim=(-1500,1500),xlabel='East (m)',ylabel='North (m)')
axs[0].set_title('DGT2018 core, no vectors');axs[1].set_title('OSM roads cyan / river blue / footprints yellow')
for k,line in osm.lines:
 a=np.asarray(line);axs[1].plot(a[:,0],a[:,1],color='cyan' if k=='road' else 'blue',lw=.5 if k=='road' else 2)
for b in osm.buildings:
 a=np.asarray(b.poly+[b.poly[0]]);axs[1].plot(a[:,0],a[:,1],color='yellow',lw=.4)
fig.suptitle('DGT2018 CC BY4.0 — Informação geográfica cedida pela Direção-Geral do Território; OSM © contributors ODbL\nHistoric imagery with current cached OSM; no arbitrary registration shift applied')
fig.savefig(root/'collab/shots/r2-w1-core-overlay.png',dpi=140);plt.close(fig)
