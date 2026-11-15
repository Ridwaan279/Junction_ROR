"""Georeferencing and collision-consistent terrain tiling invariants."""
import json
import numpy as np
import pytest
from emberwatch_drone.arganil import terrain
from emberwatch_drone.arganil.geodata import DgtOrtho,to_map


def test_dgt_geographic_pixel_centres_and_bounds():
    d=DgtOrtho.__new__(DgtOrtho);d.bounds=[-8.06,40.21,-8.05,40.22]
    d.img=np.arange(4*4*3,dtype=np.uint8).reshape(4,4,3)
    e,n=to_map(40.22-(1.5/4)*.01,-8.06+(2.5/4)*.01)
    assert np.allclose(d.rgb_map(e,n),d.img[1,2],atol=1e-6)
    with pytest.raises(ValueError):d.rgb_map(*to_map(40.3,-8.05))


def test_chunk_mesh_shares_collider_grid_and_winding(tmp_path,monkeypatch):
    monkeypatch.setattr(terrain,'MAP_HALF_M',500.)
    monkeypatch.setattr(terrain,'CORE_TEX_PX',16)
    monkeypatch.setattr(terrain,'OUTER_TEX_PX',8)
    g=np.arange(-500,501,20.);e,n=np.meshgrid(g,g);z=.1*e+.2*n
    class RGB:
        def rgb_map(self,e,n):return np.zeros((*e.shape,3))+100
    chunks=terrain.build_terrain_chunks(RGB(),RGB(),str(tmp_path),g,z)
    assert len(chunks)==4
    for chunk in chunks:
        vertices=np.array([[float(v) for v in s.split()[1:]] for s in (tmp_path/chunk['mesh']).read_text().splitlines() if s.startswith('v ')])
        assert len(vertices)==26**2
        assert np.allclose(vertices[:,2],vertices[:,0]*.1+vertices[:,1]*.2)
        assert vertices[:,0].min()==chunk['bounds'][0]
        assert vertices[:,1].max()==chunk['bounds'][3]
    assert len(json.loads((tmp_path/'terrain_chunks.json').read_text()))==4


def test_canopy_integration_marks_truthful_tree_cells(tmp_path,monkeypatch):
    from emberwatch_drone.arganil import worldgen
    monkeypatch.setattr(worldgen,'MAP_HALF_M',10.)
    class Ground:
        def z_map(self,e,n):return np.zeros_like(e)
    canopy={'res':10.,'x0':-5.,'y0':-5.,'mask':np.array([[True,False],[False,False]]),'top':np.array([[12.,0],[0,0]])}
    worldgen.build_raster(Ground(),[],[],str(tmp_path),np.array([-10.,10.]),np.zeros((2,2)),canopy=canopy)
    with np.load(tmp_path/'obstacles.npz') as data:
        assert data['top'][0,0]==12
        assert data['kind'][0,0]==2
        assert data['ground'][0,0]==0
        assert data['kind'][-1,-1]==0


def test_no_satellite_does_not_reuse_stale_chunk_manifest(tmp_path):
    from emberwatch_drone.arganil.world_sdf import write_world
    (tmp_path/'terrain_chunks.json').write_text(json.dumps([{'mesh':'old_chunk.obj','texture':'old.jpg'}]))
    write_world(str(tmp_path),169.86,chunked=False)
    text=(tmp_path/'emberwatch_arganil.sdf').read_text()
    assert 'old_chunk.obj' not in text
    assert 'terrain.obj' in text
