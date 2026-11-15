"""Thin wrappers around the documented ``gz`` CLI (no gz Python bindings are installed).

Every call is asynchronous (Popen) so ROS callbacks never block on the ~1 s gz CLI start-up.
GZ_PARTITION is inherited from the environment.
"""

from __future__ import annotations

import math
import os
import subprocess
import tempfile
from pathlib import Path
from xml.sax.saxutils import escape

_procs: list[subprocess.Popen] = []


def _puff_texture() -> str:
    # Spawned SDF is written to a temp dir, so relative texture paths would not resolve.
    try:
        from ament_index_python.packages import get_package_share_directory
        return os.path.join(get_package_share_directory("emberwatch_drone"), "worlds", "media", "puff.png")
    except Exception:  # noqa: BLE001 - allow source-tree static-scene use without ROS
        return str(Path(__file__).resolve().parent.parent / "worlds/media/puff.png")


PUFF_TEXTURE = _puff_texture()


def _run(cmd: list[str]) -> subprocess.Popen:
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    _procs.append(p)
    _procs[:] = [q for q in _procs if q.poll() is None]
    return p


def run_sync(cmd: list[str], timeout: float = 10.0) -> bool:
    try:
        return subprocess.run(cmd, capture_output=True, timeout=timeout).returncode == 0
    except (subprocess.SubprocessError, OSError):
        return False


def spawn_sdf(world: str, sdf: str, name: str, wait: bool = False) -> bool | subprocess.Popen:
    fd, path = tempfile.mkstemp(prefix=f"ember_{name}_", suffix=".sdf")
    with os.fdopen(fd, "w") as f:
        f.write(sdf)
    cmd = ["gz", "service", "-s", f"/world/{world}/create", "--reqtype", "gz.msgs.EntityFactory",
           "--reptype", "gz.msgs.Boolean", "--timeout", "5000",
           "--req", f'sdf_filename: "{path}" name: "{name}" allow_renaming: false']
    return run_sync(cmd) if wait else _run(cmd)


def remove_model(world: str, name: str) -> None:
    _run(["gz", "service", "-s", f"/world/{world}/remove", "--reqtype", "gz.msgs.Entity",
          "--reptype", "gz.msgs.Boolean", "--timeout", "3000", "--req", f'name: "{name}" type: MODEL'])


def emitter_cmd(model: str, link: str, emitter: str, fields: str) -> None:
    """fields: gz.msgs.ParticleEmitter text, e.g. 'emitting: {data: false}' or 'rate: {data: 30}'."""
    _run(["gz", "topic", "-t", f"/model/{model}/link/{link}/particle_emitter/{emitter}/cmd",
          "-m", "gz.msgs.ParticleEmitter", "-p", fields])


def pose(x: float, y: float, z: float, roll: float = 0.0, pitch: float = 0.0, yaw: float = 0.0) -> str:
    return f"{x:.3f} {y:.3f} {z:.3f} {roll:.4f} {pitch:.4f} {yaw:.4f}"


def emitter_sdf(name: str, pose_str: str, *, emitting: bool, kind: str, size: tuple, particle_size: float,
                lifetime: float, vmin: float, vmax: float, rate: float, scale_rate: float, rgb: tuple,
                texture: str | None = None, color_range_image: str | None = None) -> str:
    texture = texture or PUFF_TEXTURE
    r, g, b = rgb
    dims = (particle_size,) * 3 if isinstance(particle_size, (int, float)) else particle_size
    gradient = f"<color_range_image>{escape(color_range_image)}</color_range_image>" if color_range_image else ""
    return f"""<particle_emitter name="{name}" type="{kind}">
  <pose>{pose_str}</pose><emitting>{'true' if emitting else 'false'}</emitting>
  <size>{size[0]} {size[1]} {size[2]}</size>
  <particle_size>{dims[0]} {dims[1]} {dims[2]}</particle_size>
  <lifetime>{lifetime}</lifetime><min_velocity>{vmin}</min_velocity><max_velocity>{vmax}</max_velocity>
  <scale_rate>{scale_rate}</scale_rate><rate>{rate}</rate>
  <material><diffuse>{r} {g} {b}</diffuse><specular>{r} {g} {b}</specular>
    <pbr><metal><albedo_map>{escape(texture)}</albedo_map></metal></pbr></material>{gradient}
</particle_emitter>"""


def direction_rpy(dx: float, dy: float, dz: float) -> tuple[float, float, float]:
    """RPY that points an emitter's +X axis (its emission direction) along (dx, dy, dz) ENU."""
    yaw = math.atan2(dy, dx)
    pitch = -math.atan2(dz, math.hypot(dx, dy))
    return 0.0, pitch, yaw


def fire_asset(name: str) -> str:
    """Ensure original sprites exist; writes stay under ros2/generated.

    Both the static viewer and FireSim call this, so a clean deployment needs no
    undocumented generation step. The generator is shipped as package data.
    """
    if os.environ.get("EMBERWATCH_FIRE_MEDIA"):
        folder = Path(os.environ["EMBERWATCH_FIRE_MEDIA"]).resolve()
    else:
        root = next((p for p in Path(__file__).resolve().parents if p.name == "ros2"), None)
        if root is None:
            raise FileNotFoundError("Set EMBERWATCH_FIRE_MEDIA to an existing generated sprite folder")
        folder = root / "generated/fire"
    target = folder / name
    if not target.exists():
        generator = Path(PUFF_TEXTURE).parent / "fire/generate_assets.py"
        if not generator.is_file():
            generator = Path(__file__).resolve().parent.parent / "worlds/media/fire/generate_assets.py"
        if not generator.is_file():
            raise FileNotFoundError(f"Fire sprite generator is missing: {generator}")
        import runpy
        # generate() validates the output directory BEFORE creating any files.
        runpy.run_path(str(generator))["generate"](folder)
    return str(target)


def smoke_motion(wind_from_deg: float | None, wind_speed_mps: float | None,
                 plume_length_m: float = 25.0) -> tuple[tuple[float, float, float], float, float]:
    """Meteorological FROM -> visual downwind velocity, capped to the planning plume length."""
    speed = max(0.0, float(wind_speed_mps or 0.0)) if wind_from_deg is not None else 0.0
    angle = math.radians(float(wind_from_deg or 0.0))
    vec = (-math.sin(angle) * speed, -math.cos(angle) * speed, 1.2)
    return direction_rpy(*vec), math.hypot(speed, 1.2), min(12.0, plume_length_m / speed) if speed > 0.1 else 9.0


FIRE_FLAME_POOL = 12
_transport = None


def _fire_source(name):
    target = Path(PUFF_TEXTURE).parent / 'fire' / name
    if not target.is_file():
        target = Path(__file__).resolve().parent.parent / 'worlds/media/fire' / name
    if not target.is_file():
        raise FileNotFoundError(f'Packaged fire source missing: {target}')
    return target


def _generated_fire_dir():
    # The generator validates this directory before writing assets.
    return Path(fire_asset('flame.png')).parent


class PersistentTransport:
    """One owned native publisher/client process; no subprocess per emitter per tick.

    The small adapter uses installed Harmonic headers/libraries only. It is compiled once
    at visual startup into generated/fire. Compilation failure uses explicit CLI fallback;
    large markers are skipped with a warning rather than an OS argv overflow.
    """
    def __init__(self):
        import shlex
        import warnings
        self.process = self.log = None
        self.fallback_reason = None
        try:
            source = _fire_source('transport.cpp')
            binary = _generated_fire_dir() / 'transport'
            if not binary.exists() or binary.stat().st_mtime < source.stat().st_mtime:
                env = dict(os.environ)
                roots = [Path(p) for p in env.get('CMAKE_PREFIX_PATH','').split(':') if p]
                roots += list(Path('/opt/ros/jazzy/opt').glob('*'))
                pkg = [str(p/'lib/pkgconfig') for p in roots if (p/'lib/pkgconfig').is_dir()]
                env['PKG_CONFIG_PATH'] = ':'.join(pkg+[env.get('PKG_CONFIG_PATH','')])
                env['TMPDIR'] = str(_generated_fire_dir())
                flags = subprocess.check_output(['pkg-config','--cflags','--libs','gz-transport13','gz-msgs10'],env=env,text=True)
                temp = binary.with_name('transport_build_'+str(os.getpid()))
                command = ['g++','-O2','-std=c++17',str(source),'-o',str(temp)] + shlex.split(flags)
                for p in roots:
                    if (p/'lib').is_dir():command.append('-Wl,-rpath,'+str(p/'lib'))
                subprocess.run(command,check=True,capture_output=True,timeout=45,env=env)
                temp.replace(binary)
            self.log = open(_generated_fire_dir()/f'transport_{os.getpid()}.log','a')
            self.process = subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=self.log,stderr=self.log,text=True)
        except (OSError,subprocess.SubprocessError) as error:
            self.fallback_reason = str(error)
            warnings.warn('Persistent fire transport unavailable; CLI fallback: '+str(error),RuntimeWarning)

    def _send(self,fields):
        if self.process is not None and self.process.poll() is None:
            try:
                self.process.stdin.write('\t'.join(fields)+'\n');self.process.stdin.flush()
                return True
            except OSError as error:
                import warnings
                warnings.warn('Fire transport stopped; CLI fallback: '+str(error),RuntimeWarning)
        return False

    def publish(self,topic,msgtype,text):
        if not self._send(['P',topic,msgtype,text]):
            _run(['gz','topic','-t',topic,'-m',msgtype,'-p',text])

    def service(self,topic,reqtype,reptype,text):
        if not self._send(['S',topic,reqtype,reptype,text]):
            if len(text.encode()) > 90000:
                import warnings
                warnings.warn('Fire marker not delivered: native transport unavailable and CLI argv would overflow',RuntimeWarning)
                return
            _run(['gz','service','-s',topic,'--reqtype',reqtype,'--reptype',reptype,'--timeout','1000','--req',text])

    def close(self):
        if self.process is not None:
            self.process.stdin.close()
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.terminate();self.process.wait(timeout=2)
            self.process = None
        if self.log is not None:self.log.close();self.log=None


def ensure_fire_transport():
    global _transport
    if _transport is None:
        _transport = PersistentTransport()
    return _transport


def close_fire_transport():
    global _transport
    if _transport is not None:_transport.close();_transport=None


def emitter_pose(x,y,z,rpy):
    roll,pitch,yaw = rpy
    cr,sr = math.cos(roll/2),math.sin(roll/2)
    cp,sp = math.cos(pitch/2),math.sin(pitch/2)
    cy,sy = math.cos(yaw/2),math.sin(yaw/2)
    q=(sr*cp*cy-cr*sp*sy,cr*sp*cy+sr*cp*sy,cr*cp*sy-sr*sp*cy,cr*cp*cy+sr*sp*sy)
    return (f'pose: {{position: {{x:{x:.4f} y:{y:.4f} z:{z:.4f}}} '
            f'orientation: {{x:{q[0]:.7f} y:{q[1]:.7f} z:{q[2]:.7f} w:{q[3]:.7f}}}}}')


def flame_profile(intensity):
    i=max(0,min(1,float(intensity)))
    return {'height_m':1+3*i,'particle_size':(.45+.35*i,.75+.95*i,.5),
            'lifetime':.6+.4*i,'vmin':.25+.55*i,'vmax':.50+1.05*i,
            'rate':18+27*i}


def _marker_fields(vertices,namespace,identifier,glow=0):
    import json
    points=' '.join(f'point: {{x:{x:.4f} y:{y:.4f} z:{z:.4f}}}' for x,y,z in vertices)
    if glow:
        color=f'r:{.25*glow:.4f} g:{.035*glow:.4f} b:{.002*glow:.4f} a:0.25'
        material=f'material: {{ambient: {{{color}}} diffuse: {{{color}}} emissive: {{{color}}}}}'
    else:
        material='material: {ambient: {r:.035 g:.027 b:.018 a:1} diffuse: {r:.045 g:.033 b:.02 a:1}}'
    return (f'ns:{json.dumps(namespace)} id:{identifier} action:ADD_MODIFY type:TRIANGLE_LIST '
            'visibility:ALL pose:{orientation:{w:1}} scale:{x:1 y:1 z:1} '+material+' '+points)


class FireVisualController:
    """Fixed twelve-part surface-fire contour + dark-to-light wind smoke, visual only.

    One FireModel remains authoritative. The contour union interpolates between ignited
    seeds and may include representative connecting scars; it is not physical spread.
    """
    def __init__(self,fire,*,terrain_npz=None,spread_points=(),max_radius_m=4.0,name='sim_fire',
                 wind_from_deg=270.0,wind_speed_mps=5.0,plume_length_m=25.0,transport=None):
        import runpy
        helpers=runpy.run_path(str(_fire_source('surface.py')))
        self.fire=tuple(fire);self.name=name
        self.surface=helpers['TerrainSurface'](terrain_npz,fallback_z=fire[2])
        self.footprint=helpers['FireFootprint'](fire,spread_points,self.surface,max_radius_m)
        self.wind_from=wind_from_deg;self.wind_speed=wind_speed_mps
        self.smoke_rpy,self.smoke_speed,self.smoke_life=smoke_motion(wind_from_deg,wind_speed_mps,plume_length_m)
        self.transport=transport
        self.last_key=None;self.out=False;self.smoke_off=False
        self.namespace='emberwatch_'+name
        self.ignited=[]

    def _transport(self):
        return self.transport or ensure_fire_transport()

    def _emit(self,name,text):
        self._transport().publish(f'/model/{self.name}/link/fire_link/particle_emitter/{name}/cmd','gz.msgs.ParticleEmitter',text)

    def _marker(self,vertices,identifier,glow=0):
        self._transport().service('/marker','gz.msgs.Marker','gz.msgs.Empty',_marker_fields(vertices,self.namespace,identifier,glow))

    def sdf(self,*,radius_m=2.05,intensity=.35,ignited=()):
        import hashlib
        self.footprint.update(radius_m,ignited)
        samples=self.footprint.boundary(FIRE_FLAME_POOL)
        profile=flame_profile(intensity)
        particles=[]
        for j,(x,y,z,tangent,length) in enumerate(samples):
            particles.append(emitter_sdf(f'flames_{j:02d}',pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+.4,
                0,-math.pi/2,tangent-math.pi/2),emitting=intensity>0,kind='ellipsoid',
                size=(length,.65,.14),particle_size=profile['particle_size'],lifetime=profile['lifetime'],
                vmin=profile['vmin'],vmax=profile['vmax'],rate=profile['rate'],scale_rate=0,rgb=(1,1,1),
                texture=fire_asset('flame.png'),color_range_image=fire_asset('flame_lifetime.png')))
        center=self._center(samples)
        x,y,z=center
        for label,height,rate,size,scale,life,vscale,gradient in [
            ('smoke_base',.7,0,1.1,.4,min(3.5,self.smoke_life),.65,'smoke_base_lifetime.png'),
            ('smoke',1.0,14+18*intensity,1.6,.65,self.smoke_life,1,'smoke_soft_lifetime.png')]:
            particles.append(emitter_sdf(label,pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+height,*self.smoke_rpy),
                emitting=intensity>0 and label!='smoke_base',kind='ellipsoid',size=(2,2,.35),particle_size=size,lifetime=life,
                vmin=self.smoke_speed*.85*vscale,vmax=self.smoke_speed*1.1*vscale,rate=rate,scale_rate=scale,
                rgb=(1,1,1),texture=fire_asset('smoke_soft.png' if label=='smoke' else 'smoke.png'),color_range_image=fire_asset(gradient)))
        particles.append(emitter_sdf('embers',pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+1,*self.smoke_rpy),
            emitting=intensity>0,kind='ellipsoid',size=(2,2,.2),particle_size=.055,lifetime=min(3,self.smoke_life),
            vmin=self.smoke_speed*.8,vmax=self.smoke_speed*1.2,rate=4+10*intensity,scale_rate=0,rgb=(1,1,1),
            texture=fire_asset('ember.png'),color_range_image=fire_asset('ember_lifetime.png')))
        key=repr((self.fire,[(p['x'],p['y']) for p in ignited],radius_m,self.footprint.res,str(self.surface.g))).encode()
        mesh=_generated_fire_dir()/('scorch_'+hashlib.sha1(key).hexdigest()[:12]+'.obj')
        self.footprint.write_obj(mesh,self.fire)
        return f'''<sdf version="1.9"><model name="{escape(self.name)}"><static>true</static><pose>{pose(*self.fire)}</pose>
<link name="fire_link"><visual name="scorch"><cast_shadows>false</cast_shadows>
<geometry><mesh><uri>{escape(str(mesh))}</uri></mesh></geometry>
<material><ambient>.035 .027 .018 1</ambient><diffuse>.045 .033 .02 1</diffuse></material></visual>
<light name="fire_glow" type="point"><pose>{pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+.65)}</pose>
<cast_shadows>false</cast_shadows><diffuse>1 .25 .025 1</diffuse><specular>1 .2 .01 1</specular>
<attenuation><range>25</range><constant>.3</constant><linear>.08</linear><quadratic>.025</quadratic></attenuation>
<intensity>{3+6*intensity}</intensity></light>{''.join(particles)}</link></model></sdf>'''

    def _center(self,samples):
        if not samples:return self.fire
        import numpy as np
        x=sum(p[0] for p in samples)/len(samples);y=sum(p[1] for p in samples)/len(samples)
        # Concave fronts can have their geometric centre in an unburned bay. Keep the
        # dark smoke source / base light on the nearest active surface cell instead.
        r=int(np.clip(np.searchsorted(self.footprint.y,y)-1,0,self.footprint.active.shape[0]-1))
        c=int(np.clip(np.searchsorted(self.footprint.x,x)-1,0,self.footprint.active.shape[1]-1))
        if not self.footprint.active[r,c]:
            rr,cc=np.nonzero(self.footprint.active)
            k=np.argmin((self.footprint.X[rr,cc]-x)**2+(self.footprint.Y[rr,cc]-y)**2)
            x=float(self.footprint.X[rr[k],cc[k]]);y=float(self.footprint.Y[rr[k],cc[k]])
        return x,y,float(self.surface.sample(x,y))

    def update(self,*,radius_m,intensity,ignited=(),active=True,world='emberwatch_arganil',elapsed_s=0,smoke_expired=False):
        import numpy as np
        import json
        if not active:
            if not self.out:
                for j in range(FIRE_FLAME_POOL):self._emit(f'flames_{j:02d}','emitting:{data:false}')
                self._emit('embers','emitting:{data:false}')
                self._emit('smoke_base','emitting:{data:false}')
                self._emit('smoke','rate:{data:3}')
                self._transport().service('/marker','gz.msgs.Marker','gz.msgs.Empty',f'ns:{json.dumps(self.namespace)} id:2 action:DELETE_MARKER')
                self._light(world,0)
                self.out=True
            if smoke_expired and not self.smoke_off:
                self._emit('smoke','emitting:{data:false}')
                self.smoke_off=True
            return
        key=(round(radius_m,1),round(intensity,2),len(ignited))
        if key==self.last_key:return
        previous=self.footprint.burned.copy()
        self.footprint.update(radius_m,ignited)
        samples=self.footprint.boundary(FIRE_FLAME_POOL)
        profile=flame_profile(intensity)
        for j,(x,y,z,tangent,length) in enumerate(samples):
            fields=emitter_pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+.4,(0,-math.pi/2,tangent-math.pi/2))
            size=profile['particle_size']
            fields+=(f' emitting:{{data:true}} rate:{{data:{profile["rate"]:.2f}}} '
                f'size:{{x:{length:.3f} y:.65 z:.14}} particle_size:{{x:{size[0]:.3f} y:{size[1]:.3f} z:{size[2]:.3f}}} '
                f'lifetime:{{data:{profile["lifetime"]:.3f}}} min_velocity:{{data:{profile["vmin"]:.3f}}} '
                f'max_velocity:{{data:{profile["vmax"]:.3f}}}')
            self._emit(f'flames_{j:02d}',fields)
        x,y,z=self._center(samples)
        for label,height,rate in [('smoke_base',.7,0),('smoke',1.0,14+18*intensity),('embers',1,4+10*intensity)]:
            fields=emitter_pose(x-self.fire[0],y-self.fire[1],z-self.fire[2]+height,self.smoke_rpy)
            self._emit(label,fields+f' rate:{{data:{rate:.2f}}}')
        if self.last_key is None or np.any(previous != self.footprint.burned):
            self._marker(self.footprint.triangles(),1)
        glow=self.footprint.triangles(self.footprint.glow_mask()).copy();glow[:,2]+=.012
        self._marker(glow,2,.35+.65*intensity)
        self._light(world,(3+6*intensity)*(1+.12*math.sin(elapsed_s*8.3)),(x-self.fire[0],y-self.fire[1],z-self.fire[2]+.65))
        self.last_key=key

    def _light(self,world,intensity,position=None):
        fields=_light_fields(intensity)
        if position:fields+=' '+emitter_pose(*position,(0,0,0))
        self._transport().service(f'/world/{world}/light_config','gz.msgs.Light','gz.msgs.Boolean',fields)


def fire_visual_sdf(x,y,z,*,radius_m=2.05,intensity=.35,wind_from_deg=270.0,wind_speed_mps=5.0,
                    plume_length_m=25.0,name='sim_fire',terrain_npz=None,spread_points=()):
    controller=FireVisualController((x,y,z),terrain_npz=terrain_npz,spread_points=spread_points,name=name,
        wind_from_deg=wind_from_deg,wind_speed_mps=wind_speed_mps,plume_length_m=plume_length_m)
    return controller.sdf(radius_m=radius_m,intensity=intensity,ignited=spread_points)


def _light_fields(intensity):
    return (f'name:"fire_glow" type:POINT intensity:{intensity:.3f} is_light_off:{str(intensity<=0).lower()} '
            'diffuse:{r:1 g:.25 b:.025 a:1} specular:{r:1 g:.2 b:.01 a:1} '
            'range:25 attenuation_constant:.3 attenuation_linear:.08 attenuation_quadratic:.025')


def light_cmd(world,intensity):
    _run(['gz','service','-s',f'/world/{world}/light_config','--reqtype','gz.msgs.Light',
          '--reptype','gz.msgs.Boolean','--timeout','1000','--req',_light_fields(intensity)])


def scorch_cmd(front,namespace):
    """Legacy caller compatibility; terrain-aware code uses FireVisualController."""
    import json
    fields=(f'ns:{json.dumps(namespace)} id:1 action:ADD_MODIFY type:CYLINDER '
            f'pose:{{position:{{x:{front["x"]} y:{front["y"]} z:{front["z"]+.06}}}}} '
            f'scale:{{x:{front["width"]} y:{front["depth"]} z:.025}} '
            'material:{diffuse:{r:.045 g:.033 b:.02 a:1}}')
    _run(['gz','service','-s','/marker','--reqtype','gz.msgs.Marker','--reptype','gz.msgs.Empty','--timeout','1000','--req',fields])
