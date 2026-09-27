import { db } from "@/db";
import { sql } from "drizzle-orm";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  await db.execute(sql`select 1`);

  return (
    <main className="grid min-h-screen place-items-center px-6 py-12">
      <section className="w-full max-w-2xl rounded-3xl bg-white p-10 shadow-[0_24px_60px_rgba(16,24,40,0.12)]">
        <p className="m-0 text-sm uppercase tracking-[0.08em] text-slate-600">Starter template</p>
        <h1 className="mt-4 text-[clamp(2rem,5vw,3.25rem)] font-semibold leading-[1.05] text-slate-950">
          Arena Next.js PostgreSQL Starter
        </h1>
        <p className="mt-4 text-base text-slate-700">
          Server-rendered with Next.js after a successful PostgreSQL query through Drizzle.
        </p>
        
        <div className="mt-8 pt-8 border-t border-slate-200">
          <h2 className="text-lg font-medium text-slate-900 mb-4">Project AEGIS - Layer 1</h2>
          <p className="text-base text-slate-700 mb-6">
            High-Frequency Trading System for Binance Perpetual Futures
          </p>
          <div className="flex flex-col sm:flex-row gap-3">
            <Link 
              href="/hft-dashboard"
              className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
            >
              Access HFT Dashboard (English)
            </Link>
            <Link 
              href="/hft-dashboard/ar"
              className="inline-flex items-center px-4 py-2 border border-gray-300 text-sm font-medium rounded-md shadow-sm text-gray-700 bg-white hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
            >
              الوصول إلى لوحة التحكم (العربية)
            </Link>
          </div>
        </div>
      </section>
    </main>
  );
}
