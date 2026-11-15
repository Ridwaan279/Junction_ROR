"""Prepare a reproducible SIMULATED forest-fire snapshot, with no ROS/PX4 mission.

Uses exactly the crown/water SDF builders used by the mission visual nodes.
Four simple static quadcopter shapes are visual stand-ins, not flight vehicles.
The source scenario is read only; heights are refreshed from the current raster.
"""
import argparse
import json
from pathlib import Path
import runpy
import xml.etree.ElementTree as ET

import numpy as np

ROS2 = Path(__file__).resolve().parents[1]


def model_xml(sdf):
    return ET.tostring(ET.fromstring(sdf).find('model'), encoding='unicode')


def drone_model(index, pos):
    material = ('<material><ambient>.11 .14 .18 1</ambient>'
                '<diffuse>.20 .24 .29 1</diffuse><specular>.15 .15 .15 1</specular></material>')
    visuals = ['<visual name="body"><geometry><box><size>.55 .35 .20</size></box></geometry>'
               + material + '</visual>']
    for j, (x, y) in enumerate(((-.52,-.52),(-.52,.52),(.52,-.52),(.52,.52))):
        visuals.append(f'<visual name="rotor{j}"><pose>{x} {y} .12 0 0 0</pose>'
                       '<geometry><cylinder><radius>.32</radius><length>.025</length></cylinder></geometry>'
                       + material + '</visual>')
        visuals.append(f'<visual name="arm{j}"><pose>{x/2} {y/2} 0 0 0 {np.arctan2(y,x)}</pose>'
                       '<geometry><box><size>.75 .045 .045</size></box></geometry>'
                       + material + '</visual>')
    return (f'<model name="static_drone_standin_{index}"><static>true</static>'
            f'<pose>{pos[0]} {pos[1]} {pos[2]} 0 0 0</pose><link name="l">'
            + ''.join(visuals) + '</link></model>')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--scenario', required=True, type=Path)
    ap.add_argument('--world', type=Path, default=ROS2/'generated/arganil/emberwatch_arganil.sdf')
    ap.add_argument('--count', type=int, default=160)
    ap.add_argument('--drone-count', type=int, default=4)
    ap.add_argument('--density', type=float, default=1.)
    a = ap.parse_args()
    source_world = a.world.read_text()
    if any(m.get('name','').startswith(('crown_f_','crown_s_','static_drone_standin_','sim_water_drop_'))
           for m in ET.fromstring(source_world).findall('./world/model')):
        ap.error('--world must be the base scene without crown snapshot models')
    sc = json.loads(a.scenario.read_text()); fire = sc['fire']
    points = [dict(x=fire['enu'][0], y=fire['enu'][1], z=fire['ground_z'],
                   canopy_z=fire.get('canopy_z'), t_ignite_s=0)]
    points += sorted(fire.get('spread', []), key=lambda p: p['t_ignite_s'])
    if not 1 <= a.count <= len(points):
        ap.error(f'count must be from 1 to {len(points)}')
    if not 0 <= a.drone_count <= 4:
        ap.error('drone-count must be from 0 to 4')
    points = [dict(p) for p in points[:a.count]]
    with np.load(a.world.parent/'obstacles.npz') as raster:
        def top(x, y):
            r = int(np.clip(round((y-float(raster['y0']))/float(raster['res'])), 0, raster['top'].shape[0]-1))
            c = int(np.clip(round((x-float(raster['x0']))/float(raster['res'])), 0, raster['top'].shape[1]-1))
            return float(raster['top'][r,c])
        for p in points:
            p['canopy_z'] = top(p['x'],p['y'])
    v = runpy.run_path(str(ROS2/'src/emberwatch_drone/worlds/media/fire/crown_visuals.py'))
    wind = sc.get('wind', {})
    kwargs = dict(density=a.density, wind_from_deg=wind.get('from_deg'), wind_speed_mps=wind.get('speed_mps',0))
    models = []; smoke_count = 0
    for i, p in enumerate(points):
        models.append(model_xml(v['crown_flame_sdf'](i,p,**kwargs)))
        if i % 3 == 0:
            models.append(model_xml(v['crown_smoke_sdf'](i,p,**kwargs)))
            smoke_count += 1
    drones = []
    for i in range(a.drone_count):
        p = points[min(len(points)-1, int((i+.5)*len(points)/max(a.drone_count,1)))]
        pos = (p['x']-12+i*4, p['y']-15, p['canopy_z']+30+i*5)
        models.append(drone_model(i,pos))
        models.append(model_xml(v['water_spray_sdf'](i,pos,(p['x'],p['y']),p['canopy_z'])))
        drones.append(dict(id=i,pos=pos,target=(p['x'],p['y'],p['canopy_z'])))
    out = a.world.parent/f'r3_crown_{a.count}.sdf'
    out.write_text(source_world.replace('</world>','\n'.join(models)+'\n</world>'))
    manifest = dict(label='SIMULATED static snapshot, no mission/flight validation', source_scenario=str(a.scenario.resolve()),
                    scenario_id=sc.get('scenario_id'), world=str(out.resolve()), flames=len(points), smoke=smoke_count,
                    density=a.density, wind=wind, points=points, drones=drones, height_basis='current conservative planner top')
    out.with_suffix('.json').write_text(json.dumps(manifest,indent=2))
    print(json.dumps({k:manifest[k] for k in ('world','flames','smoke','density','scenario_id')}))


if __name__ == '__main__':
    main()
