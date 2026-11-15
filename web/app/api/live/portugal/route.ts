export const dynamic = "force-dynamic";
export async function GET() {
  const base=process.env.EMBERWATCH_API_BASE_URL||process.env.NEXT_PUBLIC_API_BASE_URL||"http://127.0.0.1:8080";
  try {
    const response=await fetch(base.replace(/\/$/,"")+"/api/live/portugal",{cache:"no-store",redirect:"error",signal:AbortSignal.timeout(220_000)});
    if(!response.ok)throw new Error("Upstream");
    const data=await response.json();
    if(data?.mode!=="live"||!Array.isArray(data.sources))throw new Error("Invalid response");
    return Response.json(data,{headers:{"Cache-Control":"no-store"}});
  }catch{return Response.json({error:"NATIONAL_DATA_UNAVAILABLE"},{status:503});}
}
