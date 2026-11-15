"""Original deterministic fire sprites; output MUST be inside ros2/generated.

No downloaded pixels. These original procedural assets are dedicated under CC0-1.0.
Usage: python3 .../generate_assets.py --out ros2/generated/fire
"""
from pathlib import Path
import argparse
import numpy as np
from PIL import Image


def generate(out: Path) -> None:
    out = out.resolve()
    ros2 = next(p for p in Path(__file__).resolve().parents if p.name == 'ros2')
    if not out.is_relative_to(ros2 / 'generated'):
        raise ValueError('generated assets must be inside ros2/generated')
    out.mkdir(parents=True, exist_ok=True)
    n = 128
    y, x = np.mgrid[-1:1:complex(n), -1:1:complex(n)]
    # A tapered, asymmetric flame with smaller edge tongues; the lifetime gradient
    # supplies its physical-looking bright-hot -> orange -> dark -> transparent ramp.
    wave = .11*np.sin(y*8) + .06*np.sin(y*19)
    width = .18 + .32*((y+1)/2)
    tongue = np.exp(-((x-wave)/width)**2 * 2.1) * np.clip(1-y*y, 0, 1)**.7
    tongue *= .74 + .20*np.sin(x*24+y*13)**2
    alpha = np.clip(tongue * 1.8 - .08, 0, 1)
    flame = np.ones((n,n,4), np.float32)
    hot = np.clip((y+1)/2,0,1)
    flame[:,:,0] = 1
    flame[:,:,1] = .22 + .72*hot
    flame[:,:,2] = .015 + .45*hot**3
    flame[:,:,3] = alpha
    Image.fromarray((flame*255).astype('uint8')).save(out/'flame.png')
    # Overlapping smooth lobes make an irregular smoke silhouette without hard cards.
    cloud = np.zeros((n,n),np.float32)
    rng = np.random.default_rng(20261115)
    for _ in range(14):
        cx,cy = rng.uniform(-.5,.5,2)
        r = rng.uniform(.18,.43)
        cloud = np.maximum(cloud, np.exp(-((x-cx)**2+(y-cy)**2)/(r*r*1.5)))
    radial = np.clip(1-(x*x+y*y),0,1)**.65
    cloud = cloud*radial
    smoke = np.ones((n,n,4),np.float32)
    smoke[:,:,3] = np.clip(cloud*.90,0,1)
    Image.fromarray((smoke*255).astype('uint8')).save(out/'smoke.png')
    # A single sorted plume uses lower per-card opacity; dense overlap otherwise
    # makes billboard intersections read as opaque rectangular slabs.
    smoke[:,:,3] = np.clip(cloud*.30,0,1)
    Image.fromarray((smoke*255).astype('uint8')).save(out/'smoke_soft.png')
    ember = np.ones((32,32,4),np.float32)
    yy,xx=np.mgrid[-1:1:32j,-1:1:32j]
    ember[:,:,3]=np.exp(-(xx*xx+yy*yy)*9)
    Image.fromarray((ember*255).astype('uint8')).save(out/'ember.png')
    stops = {
      'flame_lifetime.png': [(0,(255,252,190,0)),(.08,(255,248,160,255)),(.35,(255,170,32,245)),(.70,(218,65,10,165)),(1,(75,20,5,0))],
      'smoke_base_lifetime.png': [(0,(25,22,18,0)),(.08,(30,27,23,235)),(.45,(42,39,35,205)),(.8,(70,67,61,120)),(1,(100,96,90,0))],
      'smoke_lifetime.png': [(0,(45,41,35,0)),(.08,(65,60,53,200)),(.40,(110,106,98,155)),(.8,(160,156,148,70)),(1,(185,181,173,0))],
      'smoke_soft_lifetime.png': [(0,(28,25,21,0)),(.06,(35,31,26,235)),(.25,(57,52,45,190)),(.6,(105,100,92,100)),(.85,(155,150,140,38)),(1,(185,180,170,0))],
      'ember_lifetime.png': [(0,(255,225,110,0)),(.12,(255,180,35,255)),(.7,(255,75,5,180)),(1,(130,20,0,0))],
    }
    g=np.linspace(0,1,256)
    for name,values in stops.items():
        points=np.array([s[0] for s in values]); colors=np.array([s[1] for s in values])
        pixels=np.stack([np.interp(g,points,colors[:,c]) for c in range(4)],axis=-1)
        Image.fromarray(np.tile(pixels[None].astype('uint8'),(8,1,1))).save(out/name)
    print('generated original CC0 fire sprites and lifetime gradients:',out)


if __name__ == '__main__':
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out',type=Path,required=True)
    generate(ap.parse_args().out)
