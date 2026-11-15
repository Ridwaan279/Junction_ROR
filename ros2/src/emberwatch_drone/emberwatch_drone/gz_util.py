"""Thin wrappers around the documented ``gz`` CLI (no gz Python bindings are installed).

Every call is asynchronous (Popen) so ROS callbacks never block on the ~1 s gz CLI start-up.
GZ_PARTITION is inherited from the environment.
"""

from __future__ import annotations

import math
import os
import subprocess
import tempfile

_procs: list[subprocess.Popen] = []


def _puff_texture() -> str:
    # Spawned SDF is written to a temp dir, so relative texture paths would not resolve.
    try:
        from ament_index_python.packages import get_package_share_directory
        return os.path.join(get_package_share_directory("emberwatch_drone"), "worlds", "media", "puff.png")
    except Exception:  # noqa: BLE001 - fall back to resource-path lookup
        return "media/puff.png"


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
                texture: str | None = None) -> str:
    texture = texture or PUFF_TEXTURE
    r, g, b = rgb
    return f"""<particle_emitter name="{name}" type="{kind}">
  <pose>{pose_str}</pose><emitting>{'true' if emitting else 'false'}</emitting>
  <size>{size[0]} {size[1]} {size[2]}</size>
  <particle_size>{particle_size} {particle_size} {particle_size}</particle_size>
  <lifetime>{lifetime}</lifetime><min_velocity>{vmin}</min_velocity><max_velocity>{vmax}</max_velocity>
  <scale_rate>{scale_rate}</scale_rate><rate>{rate}</rate>
  <material><diffuse>{r} {g} {b}</diffuse><specular>{r} {g} {b}</specular>
    <pbr><metal><albedo_map>{texture}</albedo_map></metal></pbr></material>
</particle_emitter>"""


def direction_rpy(dx: float, dy: float, dz: float) -> tuple[float, float, float]:
    """RPY that points an emitter's +X axis (its emission direction) along (dx, dy, dz) ENU."""
    yaw = math.atan2(dy, dx)
    pitch = -math.atan2(dz, math.hypot(dx, dy))
    return 0.0, pitch, yaw
