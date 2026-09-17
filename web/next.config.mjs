/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  async rewrites() {
    return [
      {
        source: "/operation-api/:path*",
        destination: "http://127.0.0.1:8779/:path*",
      },
    ];
  },
};

export default nextConfig;
