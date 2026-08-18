import { shallowRef } from "vue";
import type { TrafficDto } from "@/model/dto";
import { TrafficRing, type TrafficPoint } from "./traffic-ring";

const ring = new TrafficRing(60);
export const liveTrafficPoints = shallowRef<readonly TrafficPoint[]>([]);

export function publishLiveTraffic(sample: TrafficDto): void {
  ring.push(sample);
  liveTrafficPoints.value = ring.snapshot();
}

export function resetLiveTraffic(): void { ring.clear(); liveTrafficPoints.value = []; }
