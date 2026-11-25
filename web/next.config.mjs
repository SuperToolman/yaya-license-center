/** @type {import('next').NextConfig} */
const nextConfig = {
  distDir: process.env.NEXT_DIST_DIR || ".next",
  output: "standalone",
  experimental: { proxyClientMaxBodySize: "512mb" },
  async rewrites() {
    return [
      {
        source: "/operation-api/:path*",
        destination: `${(process.env.YAYA_OPERATION_API_UPSTREAM || "http://127.0.0.1:8779").replace(/\/$/, "")}/:path*`,
      },
    ];
  },
};

export default nextConfig;
