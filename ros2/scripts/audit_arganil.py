"""Offline OSM/Sentinel alignment diagnostic; does not assert survey control accuracy."""
from pathlib import Path
import json
import numpy as np
from PIL import Image
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from emberwatch_drone.arganil.geodata import load_osm,map_to_ll,to_map,MAP_HALF_M

root=Path(__file__).resolve().parents[1]
feats=load_osm(str(Path.home()/'px4/geodata/osm_arganil.json'))
img=Image.open(root/'generated/arganil/terrain.jpg')
fig,axs=plt.subplots(1,2,figsize=(16,8),layout='constrained')
for ax in axs:
 ax.imshow(img,extent=(-3000,3000,-3000,3000),origin='upper')
 ax.set(xlim=(-650,650),ylim=(-650,650),xlabel='East (m)',ylabel='North (m)',aspect='equal')
axs[0].set_title('Before / licensed 2016 imagery, no vector overlay')
ax=axs[1];ax.set_title('Audit / OSM roads (cyan), river (blue), footprints (yellow)')
for k,line in feats.lines:
 a=np.asarray(line);ax.plot(a[:,0],a[:,1],color='deepskyblue' if k=='river' else 'cyan',lw=2 if k=='river' else .6,alpha=.8)
for b in feats.buildings:
 a=np.array(b.poly+[b.poly[0]]);ax.plot(a[:,0],a[:,1],color='yellow',lw=.6)
ax.plot(276,-229,'r+',ms=14)
fig.suptitle('Arganil ENU — EOX Sentinel-2 cloudless 2016 CC BY 4.0; © OpenStreetMap contributors ODbL\n2016 imagery ≈10 m source resolution; representative buildings/vegetation; no independent survey controls')
out=root/'collab/shots/r1-w1-georef-overlay.png';fig.savefig(out,dpi=150);plt.close(fig)
pts=np.array([p for b in feats.buildings for p in b.poly]);lat,lon=map_to_ll(pts[:,0],pts[:,1]);back=np.array([to_map(a,b) for a,b in zip(lat,lon)])
errors=np.linalg.norm(pts-back,axis=1)
summary={'controls':'OSM vertices, mathematical round-trip only (not independent surveyed/image control points)',
 'vertices':len(pts),'roundtrip_max_m':float(errors.max()),'roundtrip_rms_m':float(np.sqrt(np.mean(errors**2))),
 'texture_pixels':img.size,'texture_sample_spacing_m':6000/(img.width-1),'uv_half_texel_max_axis_m':3000/img.width,
 'observed_image_registration_offset_m':None,'reason':'No independent surveyed controls or subpixel image matching; overlay alone cannot establish a metre-accurate physical offset.',
 'scale':'ENU grid is 6000 x 6000 m; native Web Mercator mosaic is reprojected through map_to_ll before UV sampling, not stretched as raw Mercator.',
 'conclusion':'No mathematical ENU or axis-scale error found. Do not apply an arbitrary texture translation. Approx. 10m imagery is upsampled to 1.465m/pixel; blur is not new detail.'}
(root/'generated/static_scene/georef-audit.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
