"""Controlled Harmonic ogre2 SDF fog A/B (no PX4)."""
from pathlib import Path
import json,os,signal,subprocess,time,xml.etree.ElementTree as ET
from PIL import Image
import numpy as np
from static_scene import command,service,camera_request,ROS2
out=ROS2/'generated/w5_fog';out.mkdir(parents=True,exist_ok=True)
results={}
for label in ['off','on']:
 tree=ET.parse(ROS2/'generated/arganil/emberwatch_arganil.sdf');world=tree.getroot().find('world')
 for m in world.findall('model'):world.remove(m)
 scene=world.find('scene');sky=scene.find('sky')
 if sky is not None:scene.remove(sky)
 if label=='on':
  fog=ET.SubElement(scene,'fog')
  for k,v in [('type','linear'),('color','.66 .76 .88 1'),('start','5'),('end','100'),('density','.05')]:ET.SubElement(fog,k).text=v
 for i,y in enumerate([100,300,1000]):
  world.append(ET.fromstring(f'<model name="red{i}"><static>true</static><pose>{(i-1)*100} {y} 20 0 0 0</pose><link name="l"><visual name="v"><geometry><box><size>80 40 40</size></box></geometry><material><ambient>.7 .03 .02 1</ambient><diffuse>.7 .03 .02 1</diffuse></material></visual></link></model>'))
 path=out/f'{label}.sdf';tree.write(path);owned=[];handles=[]
 try:
  for name,args in [('server',['gz','sim','-r','-s',str(path)]),('gui',['gz','sim','-g'])]:
   fh=(out/f'{label}-{name}.log').open('w');handles.append(fh);owned.append(subprocess.Popen(args,stdout=fh,stderr=subprocess.STDOUT,start_new_session=True))
  deadline=time.monotonic()+60
  while '/gui/screenshot' not in command(['gz','service','-l']):
   if time.monotonic()>deadline:raise TimeoutError('GUI')
   time.sleep(.5)
  time.sleep(5);service('/gui/move_to/pose','gz.msgs.GUICamera',camera_request((0,-100,40),(0,500,20)));time.sleep(3)
  d=out/label;d.mkdir(exist_ok=True);old=set(d.glob('*.png'));service('/gui/screenshot','gz.msgs.StringMsg','data: '+json.dumps(str(d)))
  deadline=time.monotonic()+30
  while True:
   fresh=list(set(d.glob('*.png'))-old)
   try:
    if not fresh:raise OSError()
    with Image.open(fresh[-1]) as im:im.load();im.save(ROS2/f'collab/shots/r2-w5-fog-{label}.png')
    results[label]=np.asarray(Image.open(fresh[-1]));break
   except OSError:
    if time.monotonic()>deadline:raise
    time.sleep(.2)
 finally:
  for p in reversed(owned):
   if p.poll() is None:os.killpg(p.pid,signal.SIGINT)
  for p in owned:
   try:p.wait(timeout=8)
   except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
  for fh in handles:fh.close()
if results['off'].shape==results['on'].shape:
 delta=np.abs(results['off'].astype(float)-results['on'])
 report={'same_dimensions':True,'mean_abs_rgb_difference':float(delta.mean()),'fraction_pixels_difference_gt5':float(np.mean(np.max(delta,axis=2)>5)),'max_rgb_difference':float(delta.max()),'note':'Strong linear fog A/B, same camera/materials/background, no sky animation. Inspect paired PNGs; no support claim from parser alone.'}
else:report={'same_dimensions':False,'off_shape':results['off'].shape,'on_shape':results['on'].shape}
(out/'result.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
