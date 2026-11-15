"""Crown-fire visual bounds plus preserved point/fleet interfaces. No ROS/PX4 required."""
from pathlib import Path
import io
import json
import math
import runpy
import sys
import types
import importlib
import xml.etree.ElementTree as ET

import pytest
from PIL import Image

from emberwatch_drone import gz_util as gz

SOURCE=Path(__file__).resolve().parents[1]/'worlds/media/fire/crown_visuals.py'


def helpers():return runpy.run_path(str(SOURCE))


def point(i=0):return {'x':float(i*6),'y':0.,'z':0.,'canopy_z':14.,'t_ignite_s':float(i)}


def ros_module(monkeypatch,name):
    rclpy=types.ModuleType('rclpy');node=types.ModuleType('rclpy.node');node.Node=object
    executors=types.ModuleType('rclpy.executors');executors.ExternalShutdownException=type('ExternalShutdownException',(Exception,),{})
    qos=types.ModuleType('rclpy.qos')
    for key in ('DurabilityPolicy','QoSProfile','ReliabilityPolicy'):setattr(qos,key,object)
    msg=types.ModuleType('std_msgs.msg');msg.String=object
    for key,module in [('rclpy',rclpy),('rclpy.node',node),('rclpy.executors',executors),('rclpy.qos',qos),
                       ('std_msgs',types.ModuleType('std_msgs')),('std_msgs.msg',msg)]:monkeypatch.setitem(sys.modules,key,module)
    fullname='emberwatch_drone.'+name
    monkeypatch.delitem(sys.modules,fullname,raising=False)
    return importlib.import_module(fullname)


def test_flames_on_canopy_have_variety_thin_sources_and_bounded_glow():
    h=helpers();sizes=set();ramps=set();lights=0;estimated=0
    for i in range(161):
        root=ET.fromstring(h['crown_flame_sdf'](i,point(i)))
        model=root.find('model');assert model.attrib['name']==f'crown_f_{i}'
        assert float(model.findtext('pose').split()[2])==pytest.approx(12.4)
        emitters=model.findall('.//particle_emitter')
        assert [e.attrib['name'] for e in emitters][:2]==['body','tongues']
        for e in emitters:
            dims=list(map(float,e.findtext('size').split()))
            assert dims[2]<=.5 # Ogre source depth, direction-relative; prevents vertical slabs.
            estimated+=float(e.findtext('rate'))*float(e.findtext('lifetime'))
        body=emitters[0]
        sizes.add(body.findtext('particle_size'))
        ramps.add(Path(body.findtext('color_range_image')).name)
        lights+=len(root.findall('.//light'))
        assert not root.findall('.//collision')
        assert root.find('.//visual[@name="charred_canopy"]') is not None
    assert len(sizes)>100 and len(ramps)==2
    assert lights==11
    assert estimated<12500


def test_crown_smoke_single_soft_system_has_compact_particle_budget():
    h=helpers();estimated=0
    for i in range(0,161,3):
        root=ET.fromstring(h['crown_smoke_sdf'](i,point(i)))
        assert root.find('model').attrib['name']==f'crown_s_{i}'
        emitters=root.findall('.//particle_emitter')
        assert len(emitters)==1
        e=emitters[0]
        assert e.attrib['name']=='smoke'
        life=float(e.findtext('lifetime'));diam=float(e.findtext('particle_size').split()[0])
        assert life<=10
        assert diam+life*float(e.findtext('scale_rate'))<=12.3
        estimated+=life*float(e.findtext('rate'))
        assert Path(e.findtext('material/pbr/metal/albedo_map')).name=='crown_smoke_soft_v3.png'
    assert estimated<1000
    a=h['assets']();sprite=Image.open(a['crown_smoke_soft_v3.png'])
    assert sprite.getpixel((0,0))[3]==0
    assert sprite.getchannel('A').getextrema()[1]<=36
    ramp=Image.open(a['crown_smoke_ramp_v3.png'])
    assert ramp.getpixel((200,0))[0]>ramp.getpixel((20,0))[0]
    assert ramp.getpixel((255,0))[3]==0


def test_charred_cap_stays_near_canopy_and_has_no_collider():
    h=helpers();p=Path(h['assets']()['charred_canopy_v3.obj'])
    z=[float(line.split()[3]) for line in p.read_text().splitlines() if line.startswith('v ')]
    assert max(z)-1.6<=.051 # decorative overlay at most 5 cm above the input canopy Z
    assert max(z)>min(z)


def test_per_drone_water_is_small_soft_and_keeps_model_names():
    h=helpers()
    for did in range(4):
        root=ET.fromstring(h['water_spray_sdf'](did,(0,0,35),(10,0),14))
        assert root.find('model').attrib['name']==f'sim_water_drop_{did}'
        em=root.find('.//particle_emitter')
        assert em.attrib['name']=='water'
        assert max(map(float,em.findtext('particle_size').split()))<=.13
        assert float(em.findtext('rate'))==78
        assert float(em.findtext('lifetime'))>1
        assert Path(em.findtext('material/pbr/metal/albedo_map')).name=='crown_water_v3.png'
        assert not root.findall('.//collision')


def test_exact_ignited_indices_and_retry_mapping_unchanged(monkeypatch):
    cls=ros_module(monkeypatch,'forest_fire_fx').ForestFireFx
    node=cls.__new__(cls)
    seed=point(0);late=point(2);early=point(1)
    node.pts=[seed,early,late];node.spread_by_idx={0:late,1:early}
    node.state={'active':True,'intensity':.4,'spread_points_burning':1,'ignited_idx':[0]}
    node.scenario_id='expected';node.peak=0.;node.out_t=None;node.max_spawn=20
    node.flames_on=set();node.smoke_on={};node.pending={};node.out_order=[0,1,2]
    node._check_spawns=lambda:None;node._flame_sdf=lambda i:str(i);node._smoke_sdf=lambda i:str(i)
    spawned=[];node._spawn_many=lambda items:spawned.extend(items);node._remove=lambda _:None
    node._tick()
    assert {name for name,_ in spawned}=={'crown_f_0','crown_s_0','crown_f_2'}
    original=node.state
    node._on_state(types.SimpleNamespace(data=json.dumps({'scenario_id':'foreign','active':False})))
    assert node.state is original
    # Failed checked spawns remove the exact same names from pending visual sets.
    node.pending={'crown_f_2':types.SimpleNamespace(poll=lambda:0,stdout=io.StringIO('data: false'))}
    node.retries=0;node.spawned=0
    cls._check_spawns(node)
    assert 2 not in node.flames_on and node.retries==1


def test_multidrone_wetting_and_water_model_lifecycle_survive_restyle(monkeypatch):
    cls=ros_module(monkeypatch,'fire_sim_node').FireSim
    node=cls.__new__(cls);node.scenario_id='fleet_expected'
    node.waters={};node.fleet={'wet_radius_m':18,'wet_duration_s':30}
    node.fire=(0,0);node.fire_z=0;node.range_m=10;node.visuals=True
    node.spread=[dict(point(),x=0),dict(point(1),x=40)]
    node.ignited=[];node.wet_until=[-1,-1];node._now=lambda:10
    node.get_logger=lambda:types.SimpleNamespace(info=lambda _:None,warning=lambda _:None)
    node.world='test';spawned=[];removed=[]
    monkeypatch.setattr(gz,'spawn_sdf',lambda w,s,n:spawned.append((n,s)))
    monkeypatch.setattr(gz,'remove_model',lambda w,n:removed.append(n))
    def send(did,x,target,active=True,scenario='fleet_expected'):
        node._on_water(types.SimpleNamespace(data=json.dumps({'scenario_id':scenario,'drone_id':did,'active':active,'pos_enu':[x,0,35],'target_enu':[target,0]})))
    send(1,0,2);send(2,40,38)
    assert set(node.waters)=={1,2} and node.wet_until==[40,40]
    assert [n for n,_ in spawned]==['sim_water_drop_1','sim_water_drop_2']
    assert all('crown_water_v3.png' in s for _,s in spawned)
    send(9,0,0,scenario='foreign')
    assert set(node.waters)=={1,2}
    send(1,0,2,active=False)
    assert set(node.waters)=={2} and not node.water_in_range
    assert removed==['sim_water_drop_1']
