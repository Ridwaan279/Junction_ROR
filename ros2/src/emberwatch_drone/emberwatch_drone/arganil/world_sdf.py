"""Gazebo world assembly and materials."""
from __future__ import annotations

import math
import json
import os
import numpy as np
from matplotlib.path import Path as MplPath
from PIL import Image, ImageDraw
from .geodata import (DEFAULT_BUILDING_H_M, DEFAULT_LEVEL_H_M, MAP_HALF_M, MAP_ORIGIN, Building, Satellite, Terrain, load_osm)

PX4_SYSTEMS = ["physics-system\" name=\"gz::sim::systems::Physics", "user-commands-system\" name=\"gz::sim::systems::UserCommands",
               "scene-broadcaster-system\" name=\"gz::sim::systems::SceneBroadcaster", "contact-system\" name=\"gz::sim::systems::Contact",
               "imu-system\" name=\"gz::sim::systems::Imu", "air-pressure-system\" name=\"gz::sim::systems::AirPressure",
               "air-speed-system\" name=\"gz::sim::systems::AirSpeed", "apply-link-wrench-system\" name=\"gz::sim::systems::ApplyLinkWrench",
               "navsat-system\" name=\"gz::sim::systems::NavSat", "magnetometer-system\" name=\"gz::sim::systems::Magnetometer",
               "particle-emitter-system\" name=\"gz::sim::systems::ParticleEmitter"]


def mesh_model(name, out, visuals, collisions):
    vis = "".join(f'<visual name="{v[0]}"><geometry><mesh><uri>{os.path.join(out, v[1])}</uri></mesh></geometry>{v[2]}</visual>'
                  for v in visuals)
    col = "".join(f'<collision name="{c[0]}"><geometry><mesh><uri>{os.path.join(out, c[1])}</uri></mesh></geometry></collision>'
                  for c in collisions)
    return f'<model name="{name}"><static>true</static><link name="link">{vis}{col}</link></model>'


def mat(rgb, tex=None, rough=0.9, normal=None):
    r, g, b = rgb
    normal_tag = f'<normal_map type="tangent">{normal}</normal_map>' if normal else ''
    pbr = f"<pbr><metal><albedo_map>{tex}</albedo_map>{normal_tag}<roughness>{rough}</roughness><metalness>0</metalness></metal></pbr>" if tex else ""
    return (f"<material><ambient>{r} {g} {b} 1</ambient><diffuse>{r} {g} {b} 1</diffuse>"
            f"<specular>0.05 0.05 0.05 1</specular>{pbr}</material>")


def solar_lighting():
    """NOAA fractional-year approximation; representative summer demo, not event time.

    Source: https://gml.noaa.gov/grad/solcalc/solareqns.PDF
    Assumption: 2026-08-15 15:00 Europe/Lisbon (UTC+1), Arganil.
    """
    from datetime import datetime
    from zoneinfo import ZoneInfo
    dt=datetime(2026,8,15,15,tzinfo=ZoneInfo("Europe/Lisbon"))
    gamma=2*math.pi/365*(dt.timetuple().tm_yday-1+(dt.hour-12)/24)
    eq=229.18*(.000075+.001868*math.cos(gamma)-.032077*math.sin(gamma)-.014615*math.cos(2*gamma)-.040849*math.sin(2*gamma))
    dec=.006918-.399912*math.cos(gamma)+.070257*math.sin(gamma)-.006758*math.cos(2*gamma)+.000907*math.sin(2*gamma)-.002697*math.cos(3*gamma)+.00148*math.sin(3*gamma)
    solar_min=dt.hour*60+eq+4*MAP_ORIGIN[1]-dt.utcoffset().total_seconds()/60
    ha=math.radians(solar_min/4-180);lat=math.radians(MAP_ORIGIN[0])
    east=-math.cos(dec)*math.sin(ha)
    north=math.cos(lat)*math.sin(dec)-math.sin(lat)*math.cos(dec)*math.cos(ha)
    up=math.sin(lat)*math.sin(dec)+math.cos(lat)*math.cos(dec)*math.cos(ha)
    return {"label":"Representative synthetic summer lighting; not an incident event time",
            "local_time":dt.isoformat(),"azimuth_deg":math.degrees(math.atan2(east,north))%360,
            "elevation_deg":math.degrees(math.asin(up)),"ray_direction_enu":[-east,-north,-up]}


def write_world(out: str, origin_elev: float, *, chunked: bool = True) -> None:
    lighting=solar_lighting()
    json.dump(lighting,open(os.path.join(out,"lighting.json"),"w"),indent=2)
    sun_dir=" ".join(f"{v:.6f}" for v in lighting["ray_direction_enu"])
    pl = "\n".join(f'    <plugin filename="gz-sim-{p}"/>' for p in PX4_SYSTEMS)
    models = [
        mesh_model("arganil_terrain", out, [("v", "terrain.obj", mat((1, 1, 1), os.path.join(out, "terrain.jpg")))],
                   [("c", "terrain.obj")]),
        mesh_model("arganil_buildings", out, [("walls", "buildings_walls.obj", mat((1, 1, 1), os.path.join(out, "buildings_plaster.png"))),
                                              ("roofs", "buildings_roofs.obj", mat((1, 1, 1), os.path.join(out, "buildings_tiles.png")))],
                   [("walls_c", "buildings_walls.obj"), ("roofs_c", "buildings_roofs.obj")]),
        mesh_model("arganil_trees", out, [("trunks", "trees_trunks.obj", mat((1, 1, 1), os.path.join(out, "trees_bark.jpg"))),
                                          ("pine", "trees_crowns_pine.obj", mat((1, 1, 1), os.path.join(out, "trees_foliage_pine.jpg"), rough=0.95)),
                                          ("eucalyptus", "trees_crowns_eucalyptus.obj", mat((1, 1, 1), os.path.join(out, "trees_foliage_eucalyptus.jpg"), rough=0.95)),
                                          ("broadleaf", "trees_crowns_broadleaf.obj", mat((1, 1, 1), os.path.join(out, "trees_foliage_broadleaf.jpg"), rough=0.95))],
                   [("c", "trees_collision.obj")]),
    ]
    if os.path.isfile(os.path.join(out,"trees_cards.obj")) and os.path.getsize(os.path.join(out,"trees_cards.obj"))>100:
        models.append(mesh_model("arganil_leaf_cards",out,[("v","trees_cards.obj",mat((1,1,1),os.path.join(out,"trees_leaves.png")))],[]))
    if os.path.isfile(os.path.join(out,"trees_canopy.obj")) and os.path.getsize(os.path.join(out,"trees_canopy.obj"))>100:
        normal_path = os.path.join(out, 'trees_canopy_normal.png')
        models.append(mesh_model("arganil_canopy",out,[("v","trees_canopy.obj",mat((1,1,1),os.path.join(out,"trees_canopy.jpg"),normal=normal_path if os.path.isfile(normal_path) else None))],[("c","trees_canopy.obj")]))
    chunk_path = os.path.join(out, "terrain_chunks.json")
    if chunked and os.path.isfile(chunk_path):
        chunks = json.load(open(chunk_path))
        models[0] = mesh_model("arganil_terrain", out, [(f"v{i}", c["mesh"], mat((1,1,1), os.path.join(out,c["texture"]))) for i,c in enumerate(chunks)], [("c", "terrain.obj")])
    sdf = f"""<?xml version="1.0"?>
<!-- GENERATED by emberwatch_drone.arganil.worldgen - Arganil (PT) demo world. Terrain: Mapterhorn
     (DGT LiDAR DEM CC BY 4.0 / Copernicus GLO-30). Buildings/trees/landuse: (c) OpenStreetMap
     contributors, ODbL. Optional core imagery: DGT Ortofotos2018, CC BY4.0; Informação geográfica
     cedida pela Direção-Geral do Território. Outer imagery/canopy mask: EOX Sentinel-2 cloudless2016,
     CC BY4.0 (Contains modified Copernicus Sentinel data2016). Many trees and all unknown building heights are REPRESENTATIVE estimates.
     World frame: ENU metres about map origin {MAP_ORIGIN}; z = elevation - {origin_elev:.1f} m.
     spherical_coordinates is overwritten by PX4 from PX4_HOME_LAT/LON/ALT (same origin). -->
<sdf version="1.9">
  <world name="emberwatch_arganil">
    <physics type="ode"><max_step_size>0.004</max_step_size><real_time_factor>1.0</real_time_factor>
      <real_time_update_rate>250</real_time_update_rate></physics>
{pl}
    <plugin filename="gz-sim-sensors-system" name="gz::sim::systems::Sensors"><render_engine>ogre2</render_engine></plugin>
    <gravity>0 0 -9.8</gravity>
    <magnetic_field>6e-06 2.3e-05 -4.2e-05</magnetic_field>
    <atmosphere type="adiabatic"/>
    <scene><grid>false</grid><ambient>0.38 0.42 0.48 1</ambient><background>0.66 0.76 0.88 1</background>
      <shadows>true</shadows><sky><time>15</time><sunrise>6</sunrise><sunset>19</sunset><clouds><speed>2</speed><direction>1.2</direction><humidity>0.35</humidity><mean_size>0.35</mean_size></clouds></sky></scene>
    <light name="sun" type="directional"><pose>0 0 10000 0 0 0</pose><cast_shadows>true</cast_shadows>
      <intensity>0.9</intensity><direction>{sun_dir}</direction><diffuse>1 0.98 0.94 1</diffuse>
      <specular>0.2 0.2 0.2 1</specular><attenuation><range>20000</range><constant>1</constant><linear>0</linear><quadratic>0</quadratic></attenuation></light>
    {chr(10).join(models)}
    <spherical_coordinates><surface_model>EARTH_WGS84</surface_model><world_frame_orientation>ENU</world_frame_orientation>
      <latitude_deg>{MAP_ORIGIN[0]}</latitude_deg><longitude_deg>{MAP_ORIGIN[1]}</longitude_deg><elevation>{origin_elev:.2f}</elevation></spherical_coordinates>
  </world>
</sdf>
"""
    open(os.path.join(out, "emberwatch_arganil.sdf"), "w").write(sdf)


