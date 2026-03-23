import { ref, toValue, watch, type MaybeRefOrGetter, type Ref } from 'vue'
import { resolveCachedMediaUrl } from '@renderer/utils/cache'

export function useResolvedMediaUrl(
  source: MaybeRefOrGetter<string | null | undefined>,
  fallback = ''
): Ref<string> {
  const resolvedUrl = ref(fallback)
  let requestToken = 0

  watch(
    () => toValue(source),
    async (value) => {
      const currentToken = ++requestToken
      const normalizedSource = typeof value === 'string' ? value.trim() : ''

      if (!normalizedSource) {
        resolvedUrl.value = fallback
        return
      }

      resolvedUrl.value = normalizedSource
      const cachedUrl = await resolveCachedMediaUrl(normalizedSource)
      if (currentToken === requestToken) {
        resolvedUrl.value = cachedUrl || normalizedSource
      }
    },
    { immediate: true }
  )

  return resolvedUrl
}
