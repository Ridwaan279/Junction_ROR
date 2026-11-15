"""Original MIT fire footprint and exact terrain-triangle draping; illustrative only."""
from pathlib import Path
import math
import numpy as np


class TerrainSurface:
    def __init__(self, path=None, fallback_z=0.0):
        self.fallback_z = float(fallback_z)
        self.g = self.z = None
        if path:
            with np.load(path) as data:
                self.g = np.asarray(data['mesh_g'], dtype=float)
                self.z = np.asarray(data['mesh_z'], dtype=float)
            if len(self.g) < 2 or self.z.shape != (len(self.g), len(self.g)) or not np.all(np.diff(self.g) > 0):
                raise ValueError('invalid terrain mesh grid')
            if not np.allclose(np.diff(self.g), self.g[1]-self.g[0]):
                raise ValueError('fire drape requires the authoritative uniform mesh grid')

    def sample(self, x, y):
        """Exact SW→NE triangle interpolation, matching terrain OBJ face diagonals."""
        x, y = np.broadcast_arrays(np.asarray(x, float), np.asarray(y, float))
        if self.g is None:
            return np.full(x.shape, self.fallback_z)
        g, z = self.g, self.z
        if np.any((x < g[0]) | (x > g[-1]) | (y < g[0]) | (y > g[-1])):
            raise ValueError('fire footprint outside available terrain')
        c = np.clip(np.searchsorted(g, x, side='right')-1, 0, len(g)-2)
        r = np.clip(np.searchsorted(g, y, side='right')-1, 0, len(g)-2)
        u, v = (x-g[c])/(g[c+1]-g[c]), (y-g[r])/(g[r+1]-g[r])
        sw, se, ne, nw = z[r,c], z[r,c+1], z[r+1,c+1], z[r+1,c]
        return np.where(u >= v, (1-u)*sw+(u-v)*se+v*ne, (1-v)*sw+u*ne+(v-u)*nw)


class FireFootprint:
    """Nonconvex union of ignition lobes and nearest-neighbour connecting corridors.

    Connectivity is a visual interpolation, not additional model ignition. Burned area
    accumulates; active flames shrink with model radius. One fixed surface grid is used.
    """
    def __init__(self, fire, seeds=(), surface=None, max_radius=4.0):
        self.fire = tuple(fire)
        self.seeds = list(seeds)
        self.surface = surface or TerrainSurface(fallback_z=fire[2])
        pts = np.array([fire[:2]]+[(p['x'],p['y']) for p in seeds],float)
        step = float(self.surface.g[1]-self.surface.g[0]) if self.surface.g is not None else 20.0
        # Equal subdivisions align with original coarse-cell edges AND diagonals.
        # Half-metre target; coarsen by an integer subdivision only when the known
        # seed window requires it to stay within the bounded 256x256 surface.
        span = float(np.max(pts.max(0)-pts.min(0)+2*(max_radius+2)))
        desired = max(.5, span/250.0)
        self.res = step / max(1, math.floor(step/desired))
        origin = float(self.surface.g[0]) if self.surface.g is not None else 0.0
        lo = origin + np.floor((pts.min(0)-max_radius-2-origin)/self.res)*self.res
        hi = origin + np.ceil((pts.max(0)+max_radius+2-origin)/self.res)*self.res
        if self.surface.g is not None:
            lo = np.maximum(lo,self.surface.g[0]);hi=np.minimum(hi,self.surface.g[-1])
        if np.any((hi-lo)/self.res > 256):
            raise ValueError('fire footprint exceeds bounded 256x256 visual surface')
        self.x = np.arange(lo[0],hi[0]+self.res*.1,self.res)
        self.y = np.arange(lo[1],hi[1]+self.res*.1,self.res)
        self.X,self.Y = np.meshgrid((self.x[:-1]+self.x[1:])/2,(self.y[:-1]+self.y[1:])/2)
        self.burned = np.zeros(self.X.shape,bool)
        self.active = self.burned.copy()
        vx,vy = np.meshgrid(self.x,self.y)
        self.Z = self.surface.sample(vx,vy)+.045

    def update(self, radius, ignited=()):
        pts = [self.fire[:2]] + [(p['x'],p['y']) for p in ignited]
        mask = np.zeros(self.X.shape,bool)
        radius = max(.7,float(radius))
        for i,p in enumerate(pts):
            mask |= (self.X-p[0])**2+(self.Y-p[1])**2 <= radius**2
            if i:
                q = min(pts[:i],key=lambda q:(p[0]-q[0])**2+(p[1]-q[1])**2)
                dx,dy = p[0]-q[0],p[1]-q[1]
                length2 = max(dx*dx+dy*dy,1e-9)
                u = np.clip(((self.X-q[0])*dx+(self.Y-q[1])*dy)/length2,0,1)
                # A narrow connecting scar retains bays between lobes, unlike a hull/ellipse.
                mask |= (self.X-q[0]-u*dx)**2+(self.Y-q[1]-u*dy)**2 <= max(.75,radius*.55)**2
        self.active = mask
        self.burned |= mask
        return mask

    def triangles(self, mask=None):
        mask = self.burned if mask is None else mask
        r,c = np.nonzero(mask)
        # Explicit render budget: at most two triangles per bounded raster cell.
        if len(r)*2 > 131072:
            raise ValueError('draped fire marker exceeds 131072 triangle budget')
        # Every triangle lies within the authoritative terrain's corresponding plane.
        def vertex(rr,cc):
            return np.column_stack((self.x[cc],self.y[rr],self.Z[rr,cc]))
        sw,se,ne,nw = vertex(r,c),vertex(r,c+1),vertex(r+1,c+1),vertex(r+1,c)
        return np.stack((sw,se,ne,sw,ne,nw),axis=1).reshape(-1,3)

    def boundary(self, count=12):
        """Sample the longest ordered boundary, keeping concave bays in the contour."""
        edges = {}
        m = self.active;nr,nc=m.shape
        for r,c in zip(*np.nonzero(m)):
            if r==0 or not m[r-1,c]:edges[(c,r)]=(c+1,r)
            if c==nc-1 or not m[r,c+1]:edges[(c+1,r)]=(c+1,r+1)
            if r==nr-1 or not m[r+1,c]:edges[(c+1,r+1)]=(c,r+1)
            if c==0 or not m[r,c-1]:edges[(c,r+1)]=(c,r)
        loops=[]
        while edges:
            start=next(iter(edges));p=start;loop=[]
            while p in edges:
                loop.append(p);p=edges.pop(p)
                if p==start:break
            if len(loop)>2:loops.append(loop)
        if not loops:
            return []
        loop=max(loops,key=len);loop.append(loop[0])
        xy=np.array([(self.x[c],self.y[r]) for c,r in loop])
        ds=np.linalg.norm(np.diff(xy,axis=0),axis=1)
        dist=np.r_[0,np.cumsum(ds)]
        samples=np.linspace(0,dist[-1],count,endpoint=False)
        points=np.column_stack([np.interp(samples,dist,xy[:,k]) for k in (0,1)])
        result=[]
        half=max(.5,dist[-1]/count*.45)
        for d,p in zip(samples,points):
            a=np.array([np.interp((d-half)%dist[-1],dist,xy[:,k]) for k in (0,1)])
            b=np.array([np.interp((d+half)%dist[-1],dist,xy[:,k]) for k in (0,1)])
            tangent=math.atan2(*(b-a)[::-1])
            result.append((float(p[0]),float(p[1]),float(self.surface.sample(*p)),tangent,min(5.0,max(.8,dist[-1]/count))))
        return result

    def glow_mask(self):
        m=self.active
        interior=m.copy()
        interior[1:] &= m[:-1];interior[:-1] &= m[1:]
        interior[:,1:] &= m[:,:-1];interior[:,:-1] &= m[:,1:]
        return m & ~interior

    def write_obj(self,path,origin):
        vertices=self.triangles()-np.asarray(origin)
        with open(path,'w') as f:
            f.write('# Original generated scorch; terrain-draped, simulation only\n')
            for x,y,z in vertices:f.write(f'v {x:.4f} {y:.4f} {z:.4f}\n')
            f.write('vn 0 0 1\n')
            for i in range(0,len(vertices),3):f.write(f'f {i+1}//1 {i+2}//1 {i+3}//1\n')
