export const terrainCopy = {
  en: {
    back: "Back to EmberWatch", explorer: "Terrain explorer", workspace: "Incident workspace", share: "Share view", shareLabel: "Share current map view",
    hidePanel: "Hide explore panel", showPanel: "Show explore panel", searchLabel: "Search Portugal places or coordinates", search: "Search Portugal", searchButton: "Search", clear: "Clear search",
    eyebrow: "MAINLAND PORTUGAL", titleFirst: "A different", titleSecond: "point of view.", introFirst: "From the Atlantic to the highlands.", introSecond: "Explore the shape of the landscape.", relief: "Shaded relief", contours: "Contour lines", contoursPill: "Elevation contours",
    find: "Find your perspective", results: "Search results", places: "places", goCoordinates: "Go to coordinates", noResults: "No saved view matches. Try “Gerês”, “Douro”, or latitude, longitude within mainland Portugal.", footer: "Real landscapes. A little more perspective.", current: "CURRENT VIEW", dismiss: "Dismiss location detail",
    map2D: "2D map", map3D: "3D terrain", fit: "Fit mainland Portugal", north: "Reset north", zoomIn: "Zoom in", zoomOut: "Zoom out", display: "Terrain display controls", perspective: "Map perspective", settings: "The lay of the land", exaggeration: "Vertical exaggeration", tilt: "Camera tilt", orbit: "Orbit this view", pause: "Pause orbit", hint3D: "Right-drag to tilt and rotate. Scroll to explore.", hint2D: "Switch to 3D to explore the landscape in depth.",
    map: "Terrain map", unavailable: "Map unavailable", loading: "Loading the landscape…", elevation: "elevation", closer: "A little closer to the landscape.", warning: "Some elevation tiles are unavailable. The base map remains usable.", retryTitle: "Let’s get you back on the map.", loadingTitle: "Finding the lay of the land…", loadingDetail: "Loading Portugal’s map and elevation layers.", reload: "Reload map", copied: "Map link copied", manualShare: "Copy this page’s address to share the current view", pin: "Dropped pin", coordinates: "Selected coordinates", pinDetail: "Explore this location with the terrain and camera controls.", coordinatesDetail: "A location selected by latitude and longitude.", mapLabel: "Interactive terrain map of mainland Portugal",
    kinds: { mountain: "Highlands", river: "River valley", coast: "Atlantic ridge", city: "City" },
    errors: { timeout: "The map took too long to load. Check your connection and try again.", elevation: "The elevation layer could not load. Please try again.", context: "The graphics context was interrupted. Reload the map to continue.", worker: "The map renderer could not load. Reload the map to continue.", startup: "The map could not start. Check your connection and that WebGL is enabled." },
  },
  pt: {
    back: "Voltar ao EmberWatch", explorer: "Explorador de terreno", workspace: "Painel de ocorrências", share: "Partilhar vista", shareLabel: "Partilhar a vista atual do mapa",
    hidePanel: "Ocultar painel de exploração", showPanel: "Mostrar painel de exploração", searchLabel: "Pesquisar locais ou coordenadas em Portugal", search: "Pesquisar em Portugal", searchButton: "Pesquisar", clear: "Limpar pesquisa",
    eyebrow: "PORTUGAL CONTINENTAL", titleFirst: "Uma nova", titleSecond: "perspetiva.", introFirst: "Do Atlântico às serras.", introSecond: "Explore as formas da paisagem.", relief: "Relevo sombreado", contours: "Curvas de nível", contoursPill: "Curvas de nível",
    find: "Encontre a sua perspetiva", results: "Resultados da pesquisa", places: "locais", goCoordinates: "Ir para coordenadas", noResults: "Nenhuma vista guardada corresponde. Experimente “Gerês”, “Douro” ou latitude, longitude em Portugal continental.", footer: "Paisagens reais. Uma nova perspetiva.", current: "VISTA ATUAL", dismiss: "Fechar detalhes do local",
    map2D: "Mapa 2D", map3D: "Terreno 3D", fit: "Enquadrar Portugal continental", north: "Orientar para norte", zoomIn: "Aproximar", zoomOut: "Afastar", display: "Controlos de visualização do terreno", perspective: "Perspetiva do mapa", settings: "As formas do terreno", exaggeration: "Exagero vertical", tilt: "Inclinação da câmara", orbit: "Orbitar esta vista", pause: "Pausar órbita", hint3D: "Arraste com o botão direito para inclinar e rodar. Use a roda para explorar.", hint2D: "Mude para 3D para explorar a profundidade da paisagem.",
    map: "Mapa de terreno", unavailable: "Mapa indisponível", loading: "A carregar a paisagem…", elevation: "de altitude", closer: "Mais perto da paisagem.", warning: "Alguns mosaicos de altitude estão indisponíveis. O mapa base continua disponível.", retryTitle: "Vamos voltar ao mapa.", loadingTitle: "A descobrir o terreno…", loadingDetail: "A carregar o mapa e as camadas de altitude de Portugal.", reload: "Recarregar mapa", copied: "Ligação do mapa copiada", manualShare: "Copie o endereço desta página para partilhar a vista atual", pin: "Marcador colocado", coordinates: "Coordenadas selecionadas", pinDetail: "Explore este local com os controlos de terreno e de câmara.", coordinatesDetail: "Um local selecionado por latitude e longitude.", mapLabel: "Mapa de terreno interativo de Portugal continental",
    kinds: { mountain: "Serra", river: "Vale fluvial", coast: "Serra atlântica", city: "Cidade" },
    errors: { timeout: "O mapa demorou demasiado a carregar. Verifique a ligação e tente novamente.", elevation: "Não foi possível carregar a camada de altitude. Tente novamente.", context: "O contexto gráfico foi interrompido. Recarregue o mapa para continuar.", worker: "Não foi possível carregar o motor do mapa. Recarregue para continuar.", startup: "Não foi possível iniciar o mapa. Verifique a ligação e se o WebGL está ativado." },
  },
};

export const portugueseDescriptions: Record<string, string> = {
  estrela: "Cristas graníticas, vales glaciares e as terras altas do centro de Portugal.",
  geres: "Siga as formas das montanhas do norte e dos seus vales arborizados.",
  douro: "Siga o Douro pelas colinas em redor do Pinhão.",
  sintra: "A serra arborizada entre a costa atlântica e as planícies de Lisboa.",
  marvao: "Explore a escarpa granítica acima da paisagem alentejana.",
  monchique: "Descubra as serras do interior algarvio e as encostas voltadas para o Atlântico.",
  porto: "Explore a frente ribeirinha, as ruas e o relevo da foz do Douro.",
  lisboa: "As colinas e os bairros junto ao estuário do Tejo.",
};
