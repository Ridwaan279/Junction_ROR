import type { Metadata } from "next";
import TerrainExplorer from "@/components/terrain/TerrainExplorer";

export const metadata: Metadata = {
  title: "Portugal Terrain · EmberWatch",
  description: "Explore Portugal with shaded relief, elevation contours, and interactive 3D terrain.",
};

export default function TerrainPage() {
  return <TerrainExplorer />;
}
