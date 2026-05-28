export interface Channel {
  name: string;
  filename_prefix: string;
  enabled: boolean;
  regions: string[];
}

export interface HeatmapData {
  channel: string;
  weeks: number;
  weekdays: string[];
  hours: number[];
  data: number[][];      // 7×24 avg dirty systems
  observed: number[][];  // 7×24 observed hours
}

export interface SafetyData {
  channel: string;
  weeks: number;
  observed_hours: number[];  // 24 elements
  systems: { name: string; buckets: number[] }[];  // each has 24 elements (% dirty)
}

export interface SystemEntry {
  name: string;
  sightings: number;
  intervals: number;
  dirty_hours: number;
}

export interface PilotEntry {
  name: string;
  sightings: number;
  distinct_systems: number;
  last_seen: string;
}

export interface Threat {
  channel: string;
  system: string;
  started_at: string;
  ended_at: string;
}

export interface Stats {
  total_sightings: number;
  total_systems_hit: number;
  total_dirty_hours: number;
  observation_hours: number;
  top_system: string | null;
  top_pilot: string | null;
}

async function get<T>(url: string): Promise<T> {
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`${resp.status}: ${await resp.text()}`);
  return resp.json();
}

export function fetchChannels(): Promise<Channel[]> {
  return get('/api/channels');
}

export function fetchHeatmap(channel: string, weeks: number): Promise<HeatmapData> {
  return get(`/api/heatmap?channel=${encodeURIComponent(channel)}&weeks=${weeks}`);
}

export function fetchSafety(channel: string, weeks: number): Promise<SafetyData> {
  return get(`/api/safety?channel=${encodeURIComponent(channel)}&weeks=${weeks}`);
}

export function fetchSystems(channel: string, weeks: number): Promise<{ systems: SystemEntry[] }> {
  return get(`/api/systems?channel=${encodeURIComponent(channel)}&weeks=${weeks}`);
}

export function fetchPilots(channel: string, top: number): Promise<{ pilots: PilotEntry[] }> {
  return get(`/api/pilots?channel=${encodeURIComponent(channel)}&top=${top}`);
}

export function fetchCurrent(channel?: string): Promise<{ threats: Threat[] }> {
  const q = channel ? `?channel=${encodeURIComponent(channel)}` : '';
  return get(`/api/current${q}`);
}

export function fetchStats(channel: string, weeks: number): Promise<Stats> {
  return get(`/api/stats?channel=${encodeURIComponent(channel)}&weeks=${weeks}`);
}
