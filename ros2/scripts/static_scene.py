"""Fixed reproducible GUI capture of the simulated Arganil world, no flight stack."""
from pathlib import Path
import argparse
import json
import math
import os
import signal
import subprocess
import time
import numpy as np

ROS2 = Path(__file__).resolve().parents[1]

def command(args, timeout=12):
    r = subprocess.run(args, text=True, capture_output=True, timeout=timeout)
    if r.returncode:
        raise RuntimeError(f'{args}: {r.stderr}')
    return r.stdout

def service(name, reqtype, request):
    result = command(['gz','service','-s',name,'--reqtype',reqtype,'--reptype','gz.msgs.Boolean','--timeout','5000','--req',request])
    if 'data: true' not in result:
        raise RuntimeError(f'{name}: {result}')
    return result

def camera_request(eye, target):
    dx,dy,dz = np.asarray(target)-np.asarray(eye)
    yaw,pitch = math.atan2(dy,dx), math.atan2(-dz,math.hypot(dx,dy))
    cp,sp,cy,sy = math.cos(pitch/2),math.sin(pitch/2),math.cos(yaw/2),math.sin(yaw/2)
    return (f'pose: {{position: {{x:{eye[0]} y:{eye[1]} z:{eye[2]}}} '
            f'orientation: {{x:{-sp*sy} y:{sp*cy} z:{cp*sy} w:{cp*cy}}}}}')

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--world',type=Path,default=ROS2/'generated/arganil/emberwatch_arganil.sdf')
    ap.add_argument('--gui-config',type=Path,default=ROS2/'worlds/gui_mission.config')
    ap.add_argument('--fire',nargs=2,type=float,default=(276,-229),metavar=('E','N'))
    ap.add_argument('--prefix',default='r1-after')
    ap.add_argument('--cameras',nargs='+',help='capture only named presets (default all)')
    ap.add_argument('--output',type=Path,default=ROS2/'collab/shots')
    ap.add_argument('--attach',action='store_true',help='capture an already started instance in this partition')
    ap.add_argument('--window-id',help='explicit GUI X window for resize/fallback when attaching')
    ap.add_argument('--no-fire',action='store_true')
    ap.add_argument('--legacy-fire',action='store_true',help='Round 1 initial emitter comparison')
    ap.add_argument('--keep-open',action='store_true')
    ap.add_argument('--settle',type=float,default=4)
    ap.add_argument('--warmup',type=float,default=20,help='GUI mesh upload warmup before fixed-camera capture')
    ap.add_argument('--size',nargs=2,type=int,default=(1600,960),metavar=('WIDTH','HEIGHT'))
    a=ap.parse_args(); a.output.mkdir(parents=True,exist_ok=True)
    logs=ROS2/'generated/static_scene'/a.prefix;logs.mkdir(parents=True,exist_ok=True)
    owned=[]; handles=[]
    try:
        if not a.attach:
            config_source=a.gui_config if a.gui_config.is_file() else Path('/opt/ros/jazzy/opt/gz_sim_vendor/share/gz/gz-sim8/gui/gui.config')
            config=config_source.read_text()
            if '<camera_clip>' not in config:
                config=config.replace('<plugin filename="MinimalScene" name="3D View">','<plugin filename="MinimalScene" name="3D View"><horizontal_fov>90</horizontal_fov><camera_clip><near>0.15</near><far>20000</far></camera_clip>')
            gui_config=logs/'gui.config';gui_config.write_text(config)
            for name,args in [('server',['gz','sim','-r','-s',str(a.world.resolve())]),('gui',['gz','sim','-g','--gui-config',str(gui_config)])]:
                fh=(logs/f'{name}.log').open('w');handles.append(fh)
                owned.append(subprocess.Popen(args,stdout=fh,stderr=subprocess.STDOUT,start_new_session=True))
        deadline=time.monotonic()+120
        while True:
            if any(p.poll() is not None for p in owned):raise RuntimeError('Gazebo process exited; see logs')
            listing=command(['gz','service','-l'])
            if '/gui/screenshot' in listing and '/world/emberwatch_arganil/create' in listing:break
            if time.monotonic()>deadline:raise TimeoutError('Gazebo GUI services unavailable')
            time.sleep(1)
        service('/gui/camera/view_control/reference_visual','gz.msgs.Boolean','data:false')
        # Resize only the uniquely identified GUI in our isolated static scene.
        ids=command(['xdotool','search','--onlyvisible','--class','Gazebo GUI']).split()
        ids=[wid for wid in ids if command(['xdotool','getwindowname',wid]).strip()=='Gazebo Sim']
        owned_pids={p.pid for p in owned}
        ids=[wid for wid in ids if (a.window_id and wid==a.window_id) or int(command(['xdotool','getwindowpid',wid])) in owned_pids]
        window_id=ids[0] if len(ids)==1 else None
        if window_id:
            command(['xdotool','windowsize',window_id,str(a.size[0]),str(a.size[1])])
            command(['xdotool','windowactivate',window_id])
        time.sleep(a.warmup)
        import runpy
        from emberwatch_drone import gz_util as gz
        surface=runpy.run_path(str(gz._fire_source('surface.py')))['TerrainSurface'](a.world.parent/'obstacles.npz')
        def ground(x,y):return float(surface.sample(x,y))
        fx,fy=a.fire;fz=ground(fx,fy)
        with np.load(a.world.parent/'obstacles.npz') as raster:
            row=int(np.clip(round((fy-float(raster['y0']))/float(raster['res'])),0,raster['top'].shape[0]-1))
            col=int(np.clip(round((fx-float(raster['x0']))/float(raster['res'])),0,raster['top'].shape[1]-1))
            canopy_z=max(fz,float(raster['top'][row,col]))
        if not a.no_fire:
            from emberwatch_drone import gz_util as gz
            if a.legacy_fire:
                flame=gz.emitter_sdf('flames',gz.pose(0,0,.2,*gz.direction_rpy(0,0,1)),emitting=True,kind='box',size=(4.1,4.1,.2),particle_size=.7,lifetime=1.2,vmin=.8,vmax=1.8,rate=34,scale_rate=.2,rgb=(1,.45,.05))
                srpy,speed,life=gz.smoke_motion(270,5)
                smoke=gz.emitter_sdf('smoke',gz.pose(0,0,1.5,*srpy),emitting=True,kind='box',size=(2,2,.5),particle_size=1.4,lifetime=life,vmin=speed*.8,vmax=speed*1.2,rate=14,scale_rate=.7,rgb=(.33,.33,.33))
                sdf=f'<sdf version="1.9"><model name="sim_fire"><static>true</static><pose>{fx} {fy} {fz} 0 0 0</pose><link name="fire_link">{flame}{smoke}</link></model></sdf>'
            else:
                sdf=gz.fire_visual_sdf(fx,fy,fz,terrain_npz=str(a.world.parent/"obstacles.npz"))
            sf=logs/'static_fire.sdf';sf.write_text(sdf)
            service('/world/emberwatch_arganil/create','gz.msgs.EntityFactory',f'sdf_filename: {json.dumps(str(sf.resolve()))} name: "sim_fire" allow_renaming:false')
        presets={
            'town':((0,-420,ground(0,0)+300),(0,0,ground(0,0))),
            'street':((fx+55,fy-70,fz+24),(fx,fy,fz+4)),
            'forest':((850,-850,ground(850,-850)+80),(650,-480,ground(650,-480)+8)),
            'top':((0,-0.1,ground(0,0)+1000),(0,0,ground(0,0))),
            'drone-oblique':((fx+55,fy-70,ground(fx+55,fy-70)+40),(fx,fy,fz+3)),
            'town-150m':((0,-240,ground(0,-240)+150),(0,0,ground(0,0))),
            'fire-upwind':((fx-40,fy-35,ground(fx-40,fy-35)+20),(fx,fy,fz+2)),
            'fire-detail':((fx+22,fy-32,fz+8),(fx,fy,fz+3)),
            'crown-close':((fx-65,fy-80,ground(fx-65,fy-80)+65),(fx+20,fy,canopy_z+5)),
            'crown-wide':((fx-155,fy-185,ground(fx-155,fy-185)+145),(fx+35,fy+10,canopy_z+5)),
            'crown-downwind':((fx+135,fy-65,ground(fx+135,fy-65)+85),(fx+20,fy,canopy_z+9)),
        }
        if a.cameras:
            presets={name:presets[name] for name in a.cameras}
        previous_manifest=logs/'manifest.json'
        old_cameras=json.loads(previous_manifest.read_text()).get('cameras',{}) if previous_manifest.exists() else {}
        manifest={'label':'SIMULATED representative scene; fixed GUI screenshots','camera_clip_near_m':0.15,'camera_clip_far_m':20000,'camera_hfov_deg':90,'partition':os.environ.get('GZ_PARTITION'),'world':str(a.world.resolve()),'fire_enu':[fx,fy,fz],'cameras':old_cameras}
        for name,(eye,target) in presets.items():
            req=camera_request(eye,target);service('/gui/move_to/pose','gz.msgs.GUICamera',req)
            time.sleep(a.settle)
            shot=(a.output/f'{a.prefix}-{name}.png').resolve()
            capture_dir=logs/'capture'/name;capture_dir.mkdir(parents=True,exist_ok=True)
            existing=set(capture_dir.glob('*.png'))
            service('/gui/screenshot','gz.msgs.StringMsg',f'data: {json.dumps(str(capture_dir.resolve()))}')
            deadline=time.monotonic()+30
            fresh=[]
            while not fresh and time.monotonic()<deadline:
                fresh=list(set(capture_dir.glob('*.png'))-existing);time.sleep(.2)
            method='gz /gui/screenshot (directory request, timestamp PNG renamed)'
            if fresh:
                import shutil
                from PIL import Image
                source=max(fresh,key=lambda p:p.stat().st_mtime)
                deadline=time.monotonic()+10
                while True:
                    try:
                        with Image.open(source) as im:im.load()
                        break
                    except (OSError,ValueError,SyntaxError):
                        if time.monotonic()>deadline:raise
                        time.sleep(.2)
                # Keep one authoritative image; duplicate raw captures consume the
                # shared generated-assets budget without adding evidence.
                shutil.move(str(source),str(shot))
            else:
                # Some Harmonic GUI builds acknowledge screenshot requests without writing.
                # Capture the actual visible GUI window; never synthesize a renderer image.
                if not window_id:raise RuntimeError('Screenshot service produced no image, and no owned/explicit GUI window is available for fallback')
                command(['import','-window',window_id,str(shot)])
                method='ImageMagick capture of actual Gazebo GUI (service wrote no file)' 
            manifest['cameras'][name]={'eye':eye,'eye_agl_m':eye[2]-ground(eye[0],eye[1]),'target':target,'command':req,'screenshot':str(shot),'capture_method':method}
            print(shot,flush=True)
        try:manifest['stats']=command(['gz','topic','-e','-n','1','-t','/world/emberwatch_arganil/stats'],timeout=10)
        except (RuntimeError,subprocess.TimeoutExpired) as e:manifest['stats_error']=str(e)
        (logs/'manifest.json').write_text(json.dumps(manifest,indent=2))
        print(manifest.get('stats',manifest.get('stats_error')),flush=True)
        if a.keep_open and not a.attach:
            while all(p.poll() is None for p in owned):time.sleep(1)
    finally:
        if not a.no_fire and not a.legacy_fire:
            from emberwatch_drone import gz_util as gz
            gz.close_fire_transport()
        for p in reversed(owned):
            if p.poll() is None:os.killpg(p.pid,signal.SIGINT)
        for p in reversed(owned):
            try:p.wait(timeout=8)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
        for fh in handles:fh.close()
if __name__=='__main__':
    def stop_on_term(_signum,_frame):raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,stop_on_term)
    try:main()
    except KeyboardInterrupt:
        print('Static scene stopped; owned Gazebo processes cleaned up.')
