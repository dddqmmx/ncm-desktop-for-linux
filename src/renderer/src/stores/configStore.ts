// store/config.ts
import { SoundQualityType } from "@renderer/types/song";
import { defineStore } from "pinia";
import { ref, watch } from "vue";

const DEFAULT_QUALITY: SoundQualityType = "hires";
const SOUND_QUALITIES: SoundQualityType[] = ["standard", "exhigh", "lossless", "hires", "jyeffect"];

function getInitialQuality(): SoundQualityType {
  const saved = localStorage.getItem("sound_quality");
  return SOUND_QUALITIES.includes(saved as SoundQualityType) ? (saved as SoundQualityType) : DEFAULT_QUALITY;
}

export const useConfigStore = defineStore("config", () => {
  const soundQuality = ref<SoundQualityType>(getInitialQuality());

  watch(soundQuality, (newVal) => {
     localStorage.setItem("sound_quality", newVal);
  });

  function setSoundQuality(q: SoundQualityType) {
    soundQuality.value = q;
    localStorage.setItem("sound_quality", q);
  }

  return {
    soundQuality,
    setSoundQuality
  };
});
