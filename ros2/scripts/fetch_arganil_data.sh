#!/usr/bin/env bash
# Download the (small) open data used to generate the Arganil demo world, OUTSIDE the repo.
#   terrain : Mapterhorn Terrarium tiles z13 (9 x ~130 KB)  https://mapterhorn.com/attribution/
#   features: OpenStreetMap via Overpass API (~2.5 MB JSON)   (c) OpenStreetMap contributors, ODbL
#   imagery : Sentinel-2 cloudless 2016 by EOX (CC BY 4.0)    https://cloudless.eox.at
# Then:  ros2 run emberwatch_drone gen_arganil_world --data "$DATA" --out ros2/generated/arganil
set -euo pipefail
DATA="${1:-$HOME/px4/geodata}"
UA="emberwatch-sim-demo/0.1"
mkdir -p "$DATA/tiles"
cd "$DATA"
curl -sSf -A "$UA" -o attribution.json https://download.mapterhorn.com/attribution.json
for x in 3911 3912 3913; do for y in 3093 3094 3095; do
  f="tiles/13_${x}_${y}.webp"
  [[ -s $f ]] || curl -sSf -A "$UA" -o "$f" "https://tiles.mapterhorn.com/13/$x/$y.webp"
done; done
# Sentinel-2 cloudless 2016 (EOX, CC BY 4.0) - same layer as the web app; z15, 8 x 7 tiles (~0.5 MB)
mkdir -p s2
for x in $(seq 15647 15654); do for y in $(seq 12376 12382); do
  f="s2/15_${x}_${y}.jpg"
  [[ -s $f ]] || curl -sSf -A "$UA" -o "$f" "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless_3857/default/g/15/$y/$x.jpg"
done; done
cat > overpass_arganil.ql <<'Q'
[out:json][timeout:90];
(
  way["building"](40.195,-8.085,40.240,-8.025);
  relation["building"](40.195,-8.085,40.240,-8.025);
  node["natural"="tree"](40.17,-8.12,40.27,-7.99);
  way["landuse"~"forest|residential|farmland|meadow|orchard|grass"](40.17,-8.12,40.27,-7.99);
  way["natural"~"wood|scrub|heath|water"](40.17,-8.12,40.27,-7.99);
  way["waterway"="river"](40.17,-8.12,40.27,-7.99);
  way["highway"~"primary|secondary|tertiary|residential|unclassified|trunk"](40.17,-8.12,40.27,-7.99);
);
out body geom;
Q
[[ -s osm_arganil.json ]] || curl -sS -A "$UA" --max-time 120 -o osm_arganil.json \
  --data-urlencode "data@overpass_arganil.ql" https://overpass-api.de/api/interpreter
echo "data in $DATA"; ls -la "$DATA" "$DATA/tiles"
