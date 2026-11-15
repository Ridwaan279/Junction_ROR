"""Compare actual GUI top render to licensed imagery with matching camera rays/terrain."""
from pathlib import Path
import json,math,runpy
import numpy as np
from PIL import Image
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from emberwatch_drone.arganil.geodata import DgtOrtho
from emberwatch_drone import gz_util as gz
root=Path(__file__).resolve().parents[1]
manifest=json.loads((root/'generated/static_scene/r2-after/manifest.json').read_text())
record=manifest['cameras']['top'];eye=np.array(record['eye']);target=np.array(record['target'])
render=np.asarray(Image.open(record['screenshot']).convert('RGB'));height,width=render.shape[:2]
forward=(target-eye);forward=forward/np.linalg.norm(forward)
right=np.cross(forward,[0,0,1.]);right/=np.linalg.norm(right);up=np.cross(right,forward)
u=(2*(np.arange(width)+.5)/width-1)*math.tan(math.radians(manifest['camera_hfov_deg'])/2)
v=(1-2*(np.arange(height)+.5)/height)*height/width*math.tan(math.radians(manifest['camera_hfov_deg'])/2)
U,V=np.meshgrid(u,v);direction=forward+U[...,None]*right+V[...,None]*up
surface=runpy.run_path(str(gz._fire_source('surface.py')))['TerrainSurface'](root/'generated/arganil/obstacles.npz')
z=np.zeros(U.shape)
for _ in range(15):
 distance=(z-eye[2])/direction[...,2];E=eye[0]+direction[...,0]*distance;N=eye[1]+direction[...,1]*distance;z=surface.sample(E,N)
sat=DgtOrtho(str(Path.home()/'px4/geodata/dgt2018'));rgb=sat.rgb_map(E,N).clip(0,255).astype('uint8')
fig,axs=plt.subplots(1,2,figsize=(18,9),layout='constrained')
for ax,im,title in [(axs[0],render,'Gazebo GUI — representative 3D scene'),(axs[1],rgb,'DGT2018 — same camera rays intersected with terrain')]:
 ax.imshow(im);ax.set_title(title);ax.set_axis_off()
fig.suptitle('Arganil: matching top-down camera footprint and scale (HFOV90°, exact terrain triangles)\nDGT2018 CC BY4.0 — Informação geográfica cedida pela Direção-Geral do Território; OSM © contributors ODbL\nPerspective projection retained on both sides; buildings/trees add height parallax in 3D; no Google data')
fig.savefig(root/'collab/shots/r2-w5-comparison.png',dpi=130);plt.close(fig)
metadata={'hfov_degrees':manifest['camera_hfov_deg'],'image_width_height':[width,height],'eye_enu':eye.tolist(),'terrain_footprint_min_enu':[float(E.min()),float(N.min())],'terrain_footprint_max_enu':[float(E.max()),float(N.max())],'centre_ground_sample_m_per_px_approx':float(abs((E[height//2,width//2+1]-E[height//2,width//2]))),'method':'Same perspective rays/terrain intersection, exact piecewise triangle surface; not an orthographic or surveyed comparison.'}
(root/'generated/static_scene/r2-comparison.json').write_text(json.dumps(metadata,indent=2));print(json.dumps(metadata,indent=2))
