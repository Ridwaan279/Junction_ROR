"""Fire contour, exact terrain draping, lifecycle, and bounded renderer invariants."""
import importlib
import math
from pathlib import Path
import runpy
import sys
import tempfile
import types
import xml.etree.ElementTree as ET

import numpy as np
import pytest
from PIL import Image

from emberwatch_drone import gz_util as gz


@pytest.fixture
def generated_tmp():
    root=Path(__file__).resolve().parents[3]/'generated'
    root.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='fire_test_',dir=root) as folder:
        yield Path(folder)


def helper_classes():
    source=Path(__file__).resolve().parents[1]/'worlds/media/fire/surface.py'
    helpers=runpy.run_path(str(source))
    return helpers['TerrainSurface'],helpers['FireFootprint']


class Recorder:
    def __init__(self):self.emissions=[];self.services=[]
    def publish(self,*args):self.emissions.append(args)
    def service(self,*args):self.services.append(args)


def node_module(monkeypatch):
    rclpy=types.ModuleType('rclpy');node=types.ModuleType('rclpy.node');node.Node=object
    executors=types.ModuleType('rclpy.executors')
    executors.ExternalShutdownException=type('ExternalShutdownException',(Exception,),{})
    qos=types.ModuleType('rclpy.qos')
    for name in ('DurabilityPolicy','QoSProfile','ReliabilityPolicy'):setattr(qos,name,object)
    msg=types.ModuleType('std_msgs.msg');msg.String=object
    for name,module in [('rclpy',rclpy),('rclpy.node',node),('rclpy.executors',executors),
                        ('rclpy.qos',qos),('std_msgs',types.ModuleType('std_msgs')),('std_msgs.msg',msg)]:
        monkeypatch.setitem(sys.modules,name,module)
    monkeypatch.delitem(sys.modules,'emberwatch_drone.fire_sim_node',raising=False)
    return importlib.import_module('emberwatch_drone.fire_sim_node')


def test_static_sdf_fixed_pool_and_surface_fire_height():
    sdf=ET.fromstring(gz.fire_visual_sdf(276,-229,5))
    emitters=sdf.findall('.//particle_emitter')
    names=[e.attrib['name'] for e in emitters]
    assert names[:12]==[f'flames_{i:02d}' for i in range(12)]
    assert names[12:]==['smoke_base','smoke','embers']
    # One sorted plume carries dark-to-light lifetime stages. Two dense overlapping
    # systems caused opaque-looking billboard blocks in the live renderer review.
    assert emitters[12].findtext('emitting')=='false'
    assert emitters[13].findtext('emitting')=='true'
    assert Path(emitters[13].findtext('material/pbr/metal/albedo_map')).name=='smoke_soft.png'
    assert len(sdf.findall('.//light'))==1
    assert not sdf.findall('.//collision')
    for em in emitters[:12]:
        dims=list(map(float,em.findtext('size').split()))
        assert dims[2]==pytest.approx(.14) # Ogre source depth follows emission direction
        assert float(em.findtext('pose').split()[4])==pytest.approx(-math.pi/2,abs=1e-4)
        size=list(map(float,em.findtext('particle_size').split()))
        height=size[1]+float(em.findtext('max_velocity'))*float(em.findtext('lifetime'))+.4
        assert 1<height<4.1
    assert sdf.findtext('.//visual[@name="scorch"]/geometry/mesh/uri').endswith('.obj')


def test_terrain_sampler_matches_triangles_not_bilinear(generated_tmp):
    terrain,_=helper_classes()
    path=generated_tmp/'terrain.npz'
    np.savez(path,mesh_g=np.array([0.,20.]),mesh_z=np.array([[0.,0.],[0.,20.]]))
    surface=terrain(path)
    assert float(surface.sample(5,15))==5 # bilinear would incorrectly give 3.75
    assert float(surface.sample(15,5))==5
    with pytest.raises(ValueError,match='outside'):surface.sample(25,10)


def test_scorch_vertices_and_faces_drape_exactly_on_terrain_triangles(generated_tmp):
    terrain,footprint=helper_classes()
    path=generated_tmp/'terrain.npz'
    np.savez(path,mesh_g=np.array([-20.,0.,20.]),mesh_z=np.array([[0,0,0],[0,5,7],[0,8,20.]]))
    surface=terrain(path)
    front=footprint((0,0,5),surface=surface)
    assert front.res==.5
    front.update(4)
    v=front.triangles()
    assert np.allclose(v[:,2],surface.sample(v[:,0],v[:,1])+.045)
    faces=v.reshape(-1,3,3)
    mid=faces.mean(1)
    assert np.allclose(mid[:,2],surface.sample(mid[:,0],mid[:,1])+.045)


def test_large_visual_seed_window_coarsens_on_exact_mesh_subdivisions():
    _,footprint=helper_classes()
    front=footprint((0,0,0),[{'x':300,'y':200,'z':0}])
    assert front.res>.5
    assert max(front.X.shape)<=256
    assert 20/front.res==pytest.approx(round(20/front.res))
    front.update(4,[{'x':300,'y':200,'z':0}])
    assert len(front.triangles())//3<=131072


def test_nonconvex_front_keeps_bays_and_burn_area_does_not_shrink():
    _,footprint=helper_classes()
    seeds=[{'x':14,'y':0,'z':0},{'x':14,'y':14,'z':0}]
    front=footprint((0,0,0),seeds)
    front.update(2,seeds)
    assert np.count_nonzero(front.active)>30
    index=np.unravel_index(np.argmin((front.X-10)**2+(front.Y-4)**2),front.X.shape)
    assert not front.active[index] # inside a convex hull/ellipse, outside L-shaped contour
    assert len(front.boundary(12))==12
    before=front.burned.copy()
    front.update(1,seeds)
    assert np.count_nonzero(front.active)<np.count_nonzero(before)
    assert np.array_equal(front.burned,before)


def test_profile_height_and_rate_scale_with_intensity():
    low,high=gz.flame_profile(0),gz.flame_profile(1)
    assert low['height_m']==1 and high['height_m']==4
    assert low['rate']<high['rate']
    assert 12*high['rate']<=550
    assert high['particle_size'][1]+high['vmax']*high['lifetime']+.4<=4.1


def test_west_wind_drifts_east_and_unknown_vertical():
    rpy,speed,life=gz.smoke_motion(270,5,25)
    assert rpy[2]==pytest.approx(0,abs=1e-10)
    assert speed*math.cos(rpy[1])==pytest.approx(5)
    assert life==5
    rpy,speed,life=gz.smoke_motion(None,5,25)
    assert rpy[1]==pytest.approx(-math.pi/2)
    assert speed==1.2 and life==9


def test_fixed_pool_reused_and_extinction_stops_all_roles():
    recorder=Recorder()
    seeds=[{'x':i*2,'y':i,'z':0} for i in range(12)]
    controller=gz.FireVisualController((0,0,0),spread_points=seeds,transport=recorder)
    controller.update(radius_m=2.05,intensity=.35,ignited=seeds)
    assert len(recorder.emissions)==15
    flame_topics=[e[0] for e in recorder.emissions[:12]]
    assert len(set(flame_topics))==12
    assert all('z:.14}' in e[2] for e in recorder.emissions[:12])
    markers=[r for r in recorder.services if r[0]=='/marker']
    assert all(r[2]=='gz.msgs.Empty' and 'TRIANGLE_LIST' in r[3] for r in markers)
    controller.update(radius_m=2.05,intensity=.35,ignited=seeds)
    assert len(recorder.emissions)==15
    burned=controller.footprint.burned.copy()
    controller.update(radius_m=1.3,intensity=.1,ignited=seeds)
    assert np.array_equal(controller.footprint.burned,burned)
    assert [e[0] for e in recorder.emissions[15:27]]==flame_topics
    controller.update(radius_m=1,intensity=0,active=False,smoke_expired=True)
    stops=[e for e in recorder.emissions if 'emitting:{data:false}' in e[2]]
    assert len(stops)==15 # twelve flames, embers, both smoke roles
    assert any('is_light_off:true' in r[3] for r in recorder.services)
    assert any('DELETE_MARKER' in r[3] for r in recorder.services)
    count=len(recorder.emissions)
    controller.update(radius_m=1,intensity=0,active=False,smoke_expired=True)
    assert len(recorder.emissions)==count


def test_spread_never_spawns_another_model(monkeypatch):
    cls=node_module(monkeypatch).FireSim
    node=cls.__new__(cls);node.spread=[{}]*12
    node.get_logger=lambda:types.SimpleNamespace(info=lambda _:None)
    monkeypatch.setattr(gz,'spawn_sdf',lambda *a,**k:pytest.fail('spread created model'))
    for i in range(12):node._spawn_spot(i,{'x':i,'y':i})


def test_external_shutdown_is_clean_and_closes_owned_transport(monkeypatch):
    module=node_module(monkeypatch);calls=[]
    node=types.SimpleNamespace(destroy_node=lambda:calls.append('destroy'))
    monkeypatch.setattr(module,'FireSim',lambda:node)
    module.rclpy.init=lambda:None
    def spin(_):raise module.ExternalShutdownException()
    module.rclpy.spin=spin;module.rclpy.ok=lambda:False
    monkeypatch.setattr(gz,'close_fire_transport',lambda:calls.append('close'))
    module.main()
    assert calls==['close','destroy']


def test_original_assets_auto_generate_with_hot_core_and_fading_smoke(generated_tmp,monkeypatch):
    gen=runpy.run_path(str(Path(__file__).resolve().parents[1]/'worlds/media/fire/generate_assets.py'))['generate']
    with pytest.raises(ValueError,match='ros2/generated'):gen(Path('/tmp/emberwatch_forbidden_output'))
    monkeypatch.setenv('EMBERWATCH_FIRE_MEDIA',str(generated_tmp))
    assert Path(gz.fire_asset('smoke_base_lifetime.png')).is_file()
    for name in ('flame','smoke','smoke_base','ember'):
        img=Image.open(generated_tmp/f'{name}_lifetime.png')
        assert img.mode=='RGBA' and img.getpixel((255,0))[3]==0
        assert img.getpixel((80,0))[3]>0
    sprite=Image.open(generated_tmp/'flame.png')
    assert sprite.getpixel((64,100))[1]>sprite.getpixel((64,25))[1]
    soft=Image.open(gz.fire_asset('smoke_soft.png'))
    assert soft.getchannel('A').getextrema()[1]<=77
    assert soft.getpixel((0,0))[3]==0


def test_native_one_way_marker_delivery_observed_by_marker_list(generated_tmp,monkeypatch):
    """A request-only provider mirrors Gazebo GUI's marker API; assert received geometry.

    A publisher-only test cannot catch waiting for a nonexistent marker acknowledgement.
    This uses actual Gazebo transport, not the Recorder used for geometry/lifecycle tests.
    """
    import os
    import shlex
    import shutil
    import subprocess
    import time
    if not shutil.which('g++') or not shutil.which('pkg-config'):
        pytest.skip('native marker integration requires installed g++ and pkg-config')
    roots=list(Path('/opt/ros/jazzy/opt').glob('*'))
    if not roots:pytest.skip('Gazebo Harmonic development packages unavailable')
    monkeypatch.setenv('GZ_PARTITION','collab_w4')
    monkeypatch.setenv('GZ_IP','127.0.0.1')
    env=dict(os.environ)
    env['PKG_CONFIG_PATH']=':'.join(str(p/'lib/pkgconfig') for p in roots if (p/'lib/pkgconfig').is_dir())
    env['TMPDIR']=str(generated_tmp)
    flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','gz-transport13','gz-msgs10'],env=env,text=True))
    source=generated_tmp/'marker_provider.cpp'
    source.write_text(r'''
#include <gz/transport/Node.hh>
#include <gz/msgs/marker.pb.h>
#include <gz/msgs/marker_v.pb.h>
#include <gz/msgs/empty.pb.h>
#include <functional>
#include <iostream>
#include <map>
#include <mutex>
#include <string>
int main() {
  gz::transport::Node node;
  std::mutex mutex;
  std::map<unsigned long, gz::msgs::Marker> markers;
  std::function<void(const gz::msgs::Marker&)> put = [&](const auto &msg) {
    std::lock_guard<std::mutex> lock(mutex);
    markers[msg.id()] = msg;
    std::cout << "RECEIVED " << msg.ns() << " " << msg.id() << " " << msg.point_size() << std::endl;
  };
  std::function<bool(const gz::msgs::Empty&,gz::msgs::Marker_V&)> list = [&](const auto &,auto &reply) {
    std::lock_guard<std::mutex> lock(mutex);
    for (const auto &entry : markers) reply.add_marker()->CopyFrom(entry.second);
    return true;
  };
  if (!node.Advertise("/marker",put) || !node.Advertise("/marker/list",list)) return 2;
  std::cout << "READY" << std::endl;
  std::string line;
  while (std::getline(std::cin,line)) if (line == "stop") break;
}
''')
    binary=generated_tmp/'marker_provider'
    args=['g++','-O1',str(source),'-o',str(binary)]+flags
    args += ['-Wl,-rpath,'+str(p/'lib') for p in roots if (p/'lib').is_dir()]
    subprocess.run(args,env=env,check=True,capture_output=True,timeout=45)
    transport=gz.PersistentTransport()
    assert transport.process is not None,transport.fallback_reason
    log_path=generated_tmp/'provider.log'
    provider=None
    with log_path.open('w') as log:
        provider=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=log,stderr=log,text=True,env=env)
        try:
            deadline=time.monotonic()+5
            while 'READY' not in log_path.read_text():
                assert provider.poll() is None,log_path.read_text()
                assert time.monotonic()<deadline,'marker provider startup timeout'
                time.sleep(.05)
            fields='ns:"w4_oneway_probe" id:42 action:ADD_MODIFY type:TRIANGLE_LIST point:{x:1 y:2 z:3} point:{x:2 y:2 z:3} point:{x:1 y:3 z:3}'
            transport.service('/marker','gz.msgs.Marker','gz.msgs.Empty',fields)
            query=['bash','-c','source /opt/ros/jazzy/setup.bash && exec gz service -s /marker/list --reqtype gz.msgs.Empty --reptype gz.msgs.Marker_V --timeout 2000 --req ""']
            output=''
            while time.monotonic()<deadline:
                output=subprocess.run(query,capture_output=True,text=True,env=env,timeout=4).stdout
                if 'w4_oneway_probe' in output:break
                time.sleep(.1)
            assert 'w4_oneway_probe' in output,log_path.read_text()+'\n'+output
            assert 'id: 42' in output and output.count('point {')==3,output
            assert 'RECEIVED w4_oneway_probe 42 3' in log_path.read_text()
            evidence=Path(__file__).resolve().parents[3]/'generated/fire/r2_marker_delivery.log'
            evidence.write_text('NATIVE_ONEWAY_MARKER_DELIVERY_PASS\n'+log_path.read_text()+'\n/marker/list:\n'+output)
        finally:
            transport.close()
            if provider.poll() is None:
                provider.stdin.write('stop\n');provider.stdin.flush()
                try:provider.wait(timeout=3)
                except subprocess.TimeoutExpired:provider.terminate();provider.wait(timeout=3)
