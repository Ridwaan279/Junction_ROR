"""Original ROS-free crown-fire art direction. Simulation only, not fire physics.

Code MIT; original procedural textures CC0-1.0. All generated assets stay below
ros2/generated/fire/crown_v3. Lifecycle/point ignition/containment remain elsewhere.
"""
from pathlib import Path
import math
from xml.sax.saxutils import escape

from emberwatch_drone import gz_util as gz

_ASSETS = None


def assets():
    global _ASSETS
    if _ASSETS is not None and all(Path(p).is_file() for p in _ASSETS.values()):
        return _ASSETS
    from PIL import Image
    import numpy as np
    folder = Path(gz.fire_asset('flame.png')).parent/'crown_v3'
    root = next((p for p in Path(__file__).resolve().parents if p.name == 'ros2'),None)
    if root is None or not folder.resolve().is_relative_to(root/'generated'):
        raise ValueError('crown assets must remain inside ros2/generated')
    folder.mkdir(parents=True,exist_ok=True)
    names = ('crown_smoke_soft_v3.png','crown_smoke_ramp_v3.png','crown_hot_ramp_v3.png',
             'crown_red_ramp_v3.png','crown_water_v3.png','crown_water_ramp_v3.png','charred_canopy_v3.obj')
    result = {name:str(folder/name) for name in names}
    if not all(Path(p).is_file() for p in result.values()):
        n=128
        yy,xx=np.mgrid[-1:1:complex(n),-1:1:complex(n)]
        rng=np.random.default_rng(2026111503)
        cloud=np.zeros((n,n),float)
        for _ in range(18):
            cx,cy=rng.uniform(-.45,.45,2);r=rng.uniform(.17,.4)
            cloud=np.maximum(cloud,np.exp(-((xx-cx)**2+(yy-cy)**2)/(r*r*1.4)))
        # Feather the complete perimeter to zero. Low per-card opacity lets many
        # nearby crown plumes overlap without becoming large opaque grey quads.
        alpha=np.clip(cloud*np.clip(1-xx*xx-yy*yy,0,1)**1.2*.14,0,1)
        rgba=np.ones((n,n,4),float);rgba[:,:,3]=alpha
        Image.fromarray((rgba*255).astype('uint8')).save(result[names[0]])
        # A compact soft droplet, white with only a subtle blue tint in the ramp.
        dy,dx=np.mgrid[-1:1:32j,-1:1:32j]
        drop=np.ones((32,32,4),float)
        drop[:,:,3]=np.exp(-(dx*dx+dy*dy)*7)*.55
        Image.fromarray((drop*255).astype('uint8')).save(result['crown_water_v3.png'])
        ramps={
            'crown_smoke_ramp_v3.png':[(0,(35,31,26,0)),(.07,(40,35,29,220)),(.3,(67,61,52,175)),(.65,(118,112,103,85)),(1,(174,168,158,0))],
            # Small lifetime brightness pulses suggest flicker without changing
            # ROS state or adding a timer / emitter for each simulation step.
            'crown_hot_ramp_v3.png':[(0,(255,250,185,0)),(.08,(255,249,170,250)),(.22,(255,198,65,235)),(.34,(255,232,115,245)),(.52,(255,134,25,200)),(.64,(255,170,40,215)),(.82,(208,52,4,110)),(1,(65,12,0,0))],
            'crown_red_ramp_v3.png':[(0,(255,230,135,0)),(.08,(255,215,95,240)),(.23,(255,150,32,225)),(.4,(255,185,55,238)),(.57,(239,100,15,180)),(.73,(255,125,18,195)),(1,(82,16,0,0))],
            'crown_water_ramp_v3.png':[(0,(215,238,255,0)),(.08,(205,232,250,185)),(.65,(205,228,242,100)),(1,(220,237,247,0))],
        }
        t=np.linspace(0,1,256)
        for name,keys in ramps.items():
            points=np.array([v[0] for v in keys]);col=np.array([v[1] for v in keys])
            pixels=np.stack([np.interp(t,points,col[:,k]) for k in range(4)],-1)
            Image.fromarray(np.tile(pixels[None].astype('uint8'),(4,1,1))).save(result[name])
        # Low, irregular charred crown cap, shared by all point models. It is a
        # representative decorative surface, not a new obstacle or permanent scar.
        verts=[(0,0,1.65)];faces=[];count=16
        for j in range(count):
            a=j*2*math.pi/count;r=3.1*(1+.10*math.sin(j*2.7))
            verts.append((r*math.cos(a),r*math.sin(a),.35+.14*math.sin(j*1.9)))
        for j in range(count):faces.append((1,j+2,(j+1)%count+2))
        with open(result['charred_canopy_v3.obj'],'w') as f:
            f.write('# Original charred canopy estimate; MIT geometry; visual only\n')
            for x,y,z in verts:f.write(f'v {x:.5f} {y:.5f} {z:.5f}\n')
            f.write('vn 0 0 1\n')
            for a,b,c in faces:f.write(f'f {a}//1 {b}//1 {c}//1\n')
    _ASSETS=result
    return result


def _unit(index):
    return ((int(index)*7919+1237)%1009)/1008


def _base_z(point):
    canopy=point.get('canopy_z')
    return float(canopy)-1.6 if canopy is not None else float(point.get('z',0))+.35


def _wind(wind_from_deg,wind_speed_mps,rise):
    speed=min(12,max(0,float(wind_speed_mps or 0))) if wind_from_deg is not None else 0
    angle=math.radians(float(wind_from_deg or 0))
    vector=(-math.sin(angle)*speed,-math.cos(angle)*speed,rise)
    return gz.direction_rpy(*vector),math.hypot(speed,rise)


def _char_visual(index,scale=1):
    a=assets();tone=.025+.014*_unit(index)
    return (f'<visual name="charred_canopy"><cast_shadows>false</cast_shadows>'
        f'<geometry><mesh><uri>{escape(a["charred_canopy_v3.obj"])}</uri><scale>{scale:.3f} {scale:.3f} 1</scale></mesh></geometry>'
        f'<material><ambient>{tone:.4f} {tone*.8:.4f} {tone*.6:.4f} 1</ambient>'
        f'<diffuse>{tone*1.1:.4f} {tone*.85:.4f} {tone*.6:.4f} 1</diffuse><specular>.015 .015 .015 1</specular></material></visual>')


def crown_flame_sdf(index,point,*,density=1.,flame_scale=1.,wind_from_deg=270.,wind_speed_mps=5.):
    a=assets();u=_unit(index);d=max(0,min(2,float(density)));s=max(.1,float(flame_scale))
    # Twelve-metre-class crown flames vary by point. Source depth stays thin;
    # a large vertical emission volume made the previous front look like a wall.
    up=gz.pose(0,0,.35,0,-math.pi/2,u*2*math.pi)
    lean,_=_wind(wind_from_deg,wind_speed_mps,.9+u*.3)
    ramp=a['crown_red_ramp_v3.png' if index%4==0 else 'crown_hot_ramp_v3.png']
    body=gz.emitter_sdf('body',up,emitting=True,kind='ellipsoid',size=(4.6+u*1.4,4.3+u*1.8,.45),
        particle_size=((2.4+u*1.4)*s,(4.2+u*2.8)*s,1),lifetime=1.15+u*.45,
        vmin=1.8+u*.8,vmax=3.6+u*1.6,rate=round((22+u*7)*d,2),scale_rate=.25,rgb=(1,1,1),
        texture=gz.fire_asset('flame.png'),color_range_image=ramp)
    tongues=gz.emitter_sdf('tongues',gz.pose(.4*math.sin(index),.4*math.cos(index),2.2,*lean),emitting=True,
        kind='ellipsoid',size=(2.6,3.1,.3),particle_size=((1.1+u*.65)*s,(2.7+u*2)*s,1),
        lifetime=.75+u*.3,vmin=2.5,vmax=4.4+u*1.2,rate=round((11+u*5)*d,2),scale_rate=.18,rgb=(1,1,1),
        texture=gz.fire_asset('flame.png'),color_range_image=a['crown_hot_ramp_v3.png'])
    embers=''
    if index%3==0:
        erpy,espeed=_wind(wind_from_deg,wind_speed_mps,3.5)
        embers=gz.emitter_sdf('embers',gz.pose(0,0,3,*erpy),emitting=True,kind='ellipsoid',size=(4,4,.3),
            particle_size=.085,lifetime=3.2,vmin=espeed*.7,vmax=espeed*1.15,rate=round(7*d,2),scale_rate=0,
            rgb=(1,1,1),texture=gz.fire_asset('ember.png'),color_range_image=gz.fire_asset('ember_lifetime.png'))
    light=''
    if index%16==0:
        light=('<light name="glow" type="point"><pose>0 0 2.2 0 0 0</pose><cast_shadows>false</cast_shadows>'
            '<diffuse>1 .28 .025 1</diffuse><specular>.5 .12 .015 1</specular>'
            '<attenuation><range>28</range><constant>.3</constant><linear>.06</linear><quadratic>.008</quadratic></attenuation>'
            f'<intensity>{3.5+u*2:.3f}</intensity></light>')
    return (f'<sdf version="1.9"><model name="crown_f_{int(index)}"><static>true</static>'
        f'<pose>{gz.pose(point["x"],point["y"],_base_z(point))}</pose><link name="l">'
        f'{_char_visual(index,.85+u*.3)}{light}{body}{tongues}{embers}</link></model></sdf>')


def crown_smoke_sdf(index,point,*,density=1.,flame_scale=1.,wind_from_deg=270.,wind_speed_mps=5.):
    del flame_scale
    a=assets();u=_unit(index);d=max(0,min(2,float(density)))
    rpy,speed=_wind(wind_from_deg,wind_speed_mps,3.0+u*1.2)
    em=gz.emitter_sdf('smoke',gz.pose(0,0,5.5+u,*rpy),emitting=True,kind='ellipsoid',
        size=(4.4,4.4,.5),particle_size=3.1+u*.7,lifetime=8.0+u*2,
        vmin=speed*.82,vmax=speed*1.06,rate=round((1.25+u*.55)*d,2),scale_rate=.72+u*.12,rgb=(1,1,1),
        texture=a['crown_smoke_soft_v3.png'],color_range_image=a['crown_smoke_ramp_v3.png'])
    return (f'<sdf version="1.9"><model name="crown_s_{int(index)}"><static>true</static>'
        f'<pose>{gz.pose(point["x"],point["y"],_base_z(point))}</pose><link name="l">'
        f'{_char_visual(index,.85+u*.3)}{em}</link></model></sdf>')


def water_emitter_sdf(direction,distance_m):
    a=assets();dist=max(.5,float(distance_m))
    return gz.emitter_sdf('water',gz.pose(0,0,0,*gz.direction_rpy(*direction)),emitting=True,
        kind='cylinder',size=(.28,.28,.14),particle_size=(.085,.13,.085),
        lifetime=round(min(5.,max(.08,dist/10.5)),3),vmin=10.,vmax=11.,rate=78,scale_rate=.10,rgb=(1,1,1),
        texture=a['crown_water_v3.png'],color_range_image=a['crown_water_ramp_v3.png'])


def water_spray_sdf(drone_id,pos,target,target_z):
    x,y,z=pos
    vector=(target[0]-x,target[1]-y,-max(z-.5-float(target_z),.5))
    dist=math.sqrt(sum(v*v for v in vector))
    em=water_emitter_sdf(vector,dist)
    return (f'<sdf version="1.9"><model name="sim_water_drop_{int(drone_id)}"><static>true</static>'
        f'<pose>{gz.pose(x,y,z-.5)}</pose><link name="l">{em}</link></model></sdf>')
