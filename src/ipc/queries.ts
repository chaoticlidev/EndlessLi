import {
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { commands } from "./bindings";
import type { AppError, NewNote, NoteScope } from "./bindings";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 5_000 },
  },
});

export type IpcError = AppError | Error;

function unwrap<T>(
  result: { status: "ok"; data: T } | { status: "error"; error: AppError },
): T {
  if (result.status === "ok") return result.data;
  throw result.error;
}

async function call<T>(
  fn: () => Promise<
    { status: "ok"; data: T } | { status: "error"; error: AppError }
  >,
): Promise<T> {
  return unwrap(await fn());
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  const err = error as AppError;
  return err.message ?? "Unknown error";
}

/** Scope key must be stable for React Query caching. */
export function scopeKey(scope: NoteScope): string {
  switch (scope.type) {
    case "inbox":
      return "inbox";
    case "all":
      return "all";
    case "project":
      return `project:${scope.project_id}`;
    case "folder":
      return `folder:${scope.folder_id}`;
  }
}

export function useProjects(includeArchived = true) {
  return useQuery({
    queryKey: ["projects", includeArchived],
    queryFn: () => call(() => commands.projectList(includeArchived)),
  });
}

export function useFolderTree(projectId: string | null) {
  return useQuery({
    queryKey: ["folders", projectId],
    queryFn: () => call(() => commands.folderTree(projectId ?? "")),
    enabled: projectId !== null,
  });
}

export function useNotes(scope: NoteScope) {
  return useQuery({
    queryKey: ["notes", scopeKey(scope)],
    queryFn: () => call(() => commands.noteList(scope)),
  });
}

export function useNoteDetail(id: string | null) {
  return useQuery({
    queryKey: ["note-detail", id],
    queryFn: () => call(() => commands.noteDetail(id ?? "")),
    enabled: id !== null,
  });
}

export function useTags() {
  return useQuery({
    queryKey: ["tags"],
    queryFn: () => call(() => commands.tagList()),
  });
}

export function useNoteSearch(query: string, limit = 20) {
  const trimmed = query.trim();
  return useQuery({
    queryKey: ["note-search", trimmed, limit],
    queryFn: () => call(() => commands.noteSearch(trimmed, limit)),
    enabled: trimmed.length > 0,
  });
}

export function useTitleMatches(title: string, limit = 5) {
  const trimmed = title.trim();
  return useQuery({
    queryKey: ["title-matches", trimmed, limit],
    queryFn: () => call(() => commands.noteFindByTitle(trimmed, limit)),
    enabled: trimmed.length > 0,
  });
}

function invalidateKnowledge(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: ["projects"] });
  void queryClient.invalidateQueries({ queryKey: ["folders"] });
  void queryClient.invalidateQueries({ queryKey: ["notes"] });
  void queryClient.invalidateQueries({ queryKey: ["tags"] });
  void queryClient.invalidateQueries({ queryKey: ["note-detail"] });
  void queryClient.invalidateQueries({ queryKey: ["note-search"] });
}

export function useCreateNote() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: NewNote) => call(() => commands.noteCreate(input)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useUpdateNote() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; title?: string; content?: string }) =>
      call(() =>
        commands.noteUpdate(args.id, {
          title: args.title ?? null,
          content: args.content ?? null,
        }),
      ),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["notes"] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.id] });
      void client.invalidateQueries({ queryKey: ["note-search"] });
    },
  });
}

export function useMoveNote() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      id: string;
      projectId: string | null;
      folderId: string | null;
    }) => call(() => commands.noteMove(args.id, args.projectId, args.folderId)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useDeleteNote() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.noteDelete(id)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useBranchNote() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; title?: string }) =>
      call(() => commands.noteBranch(args.id, args.title ?? null)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useCreateProject() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (name: string) =>
      call(() =>
        commands.projectCreate({
          name,
          description: null,
          icon: null,
          color: null,
          system_prompt: null,
        }),
      ),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useArchiveProject() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; archived: boolean }) =>
      call(() => commands.projectArchive(args.id, args.archived)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useDeleteProject() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.projectDelete(id)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useCreateFolder() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      name: string;
      projectId: string;
      parentId: string | null;
    }) =>
      call(() =>
        commands.folderCreate({
          name: args.name,
          project_id: args.projectId,
          parent_id: args.parentId,
        }),
      ),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useRenameFolder() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: string; name: string }) =>
      call(() => commands.folderRename(args.id, args.name)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useDeleteFolder() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.folderDelete(id)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useCreateTag() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => call(() => commands.tagCreate(name, null)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useAssignTag() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { noteId: string; tagId: string }) =>
      call(() => commands.tagAssign(args.noteId, args.tagId)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["tags"] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.noteId] });
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

export function useUnassignTag() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { noteId: string; tagId: string }) =>
      call(() => commands.tagUnassign(args.noteId, args.tagId)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["tags"] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.noteId] });
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

export function useDeleteTag() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.tagDelete(id)),
    onSuccess: () => invalidateKnowledge(client),
  });
}

export function useCreateLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { a: string; b: string }) =>
      call(() => commands.linkCreateManual(args.a, args.b)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["note-detail", args.a] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.b] });
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

export function useSetting(key: string) {
  return useQuery({
    queryKey: ["setting", key],
    queryFn: () => call(() => commands.settingsGet(key)),
  });
}

export function useSetSetting() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { key: string; value: string }) =>
      call(() => commands.settingsUpdate(args.key, args.value)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["setting", args.key] });
      void client.invalidateQueries({ queryKey: ["token-summary"] });
    },
  });
}

export function useKeyStatus() {
  return useQuery({
    queryKey: ["key-status"],
    queryFn: () => call(() => commands.apiKeyStatus()),
  });
}

export function useMemoryList(projectId: string | null) {
  return useQuery({
    queryKey: ["memory-list", projectId],
    queryFn: () => call(() => commands.memoryList(projectId)),
  });
}

export function useForgottenMemories() {
  return useQuery({
    queryKey: ["memory-forgotten"],
    queryFn: () => call(() => commands.memoryListForgotten()),
  });
}

export function useMemoryEnabled() {
  return useQuery({
    queryKey: ["memory-enabled"],
    queryFn: () => call(() => commands.memoryIsEnabled()),
  });
}

export function useCreateMemory() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      memoryType: string;
      projectId: string | null;
      content: string;
      expiresAt: number | null;
    }) =>
      call(() =>
        commands.memoryCreate(
          args.memoryType,
          args.projectId,
          args.content,
          args.expiresAt,
        ),
      ),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["memory-list"] });
      void client.invalidateQueries({ queryKey: ["memory-forgotten"] });
    },
  });
}

export function useForgetMemory() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.memoryForget(id)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["memory-list"] });
      void client.invalidateQueries({ queryKey: ["memory-forgotten"] });
    },
  });
}

export function useRestoreMemory() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.memoryRestore(id)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["memory-list"] });
      void client.invalidateQueries({ queryKey: ["memory-forgotten"] });
    },
  });
}

export function useDeleteMemory() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => call(() => commands.memoryDelete(id)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["memory-list"] });
      void client.invalidateQueries({ queryKey: ["memory-forgotten"] });
    },
  });
}

export function useSetMemoryEnabled() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) =>
      call(() => commands.memorySetEnabled(enabled)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["memory-enabled"] });
      void client.invalidateQueries({ queryKey: ["memory-list"] });
    },
  });
}

export function useSetKey() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { provider: string; key: string }) =>
      call(() => commands.apiKeySet(args.provider, args.key)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["key-status"] });
    },
  });
}

export function useDeleteKey() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (provider: string) =>
      call(() => commands.apiKeyDelete(provider)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["key-status"] });
    },
  });
}

export function useTokenSummary() {
  return useQuery({
    queryKey: ["token-summary"],
    queryFn: () => call(() => commands.tokenUsageSummary()),
  });
}

export function useCacheStats() {
  return useQuery({
    queryKey: ["cache-stats"],
    queryFn: () => call(() => commands.cacheStats()),
  });
}

export function useClearCache() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (entryType: string | null) =>
      call(() => commands.cacheClear(entryType)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["cache-stats"] });
    },
  });
}

export function useRemoveLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { a: string; b: string }) =>
      call(() => commands.linkRemove(args.a, args.b)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["note-detail", args.a] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.b] });
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

export function useResearchHistory(limit = 50) {
  return useQuery({
    queryKey: ["research-history", limit],
    queryFn: () => call(() => commands.researchHistory(limit)),
  });
}

export function useResearchView(sessionId: string | null) {
  return useQuery({
    queryKey: ["research-view", sessionId],
    queryFn: () => call(() => commands.researchGet(sessionId ?? "")),
    enabled: sessionId !== null,
    refetchInterval: (query) => {
      const status = query.state.data?.session.status;
      return status === "in_progress" || status === "pending" ? 1500 : false;
    },
  });
}

export function useStartResearch() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      query: string;
      mode: string;
      projectId: string | null;
      folderId: string | null;
      sessionMemory: string | null;
    }) =>
      call(() =>
        commands.researchStart(
          args.query,
          args.mode,
          args.projectId,
          args.folderId,
          args.sessionMemory,
        ),
      ),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["research-history"] });
    },
  });
}

export function useCancelResearch() {
  return useMutation({
    mutationFn: (sessionId: string) =>
      call(() => commands.researchCancel(sessionId)),
  });
}

export function useSaveResults() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      sessionId: string;
      title: string;
      projectId: string | null;
      folderId: string | null;
      resultIds: string[];
    }) =>
      call(() =>
        commands.researchSaveResults(
          args.sessionId,
          args.title,
          args.projectId,
          args.folderId,
          args.resultIds,
        ),
      ),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

/** Auto-linking candidates for a note (top matches above the threshold). */
export function useLinkSuggestions(noteId: string) {
  return useQuery({
    queryKey: ["link-suggestions", noteId],
    queryFn: () => call(() => commands.linkSuggestions(noteId)),
  });
}

export function useApproveLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { a: string; b: string; similarity?: number | null }) =>
      call(() => commands.linkApprove(args.a, args.b, args.similarity ?? null)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["note-detail", args.a] });
      void client.invalidateQueries({ queryKey: ["note-detail", args.b] });
      void client.invalidateQueries({ queryKey: ["link-suggestions", args.a] });
      void client.invalidateQueries({ queryKey: ["link-suggestions", args.b] });
      void client.invalidateQueries({ queryKey: ["notes"] });
    },
  });
}

export function useRejectLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (args: { a: string; b: string }) =>
      call(() => commands.linkReject(args.a, args.b)),
    onSuccess: (_data, args) => {
      void client.invalidateQueries({ queryKey: ["link-suggestions", args.a] });
      void client.invalidateQueries({ queryKey: ["link-suggestions", args.b] });
    },
  });
}

/** Active embedding model, availability, and note coverage. */
export function useEmbeddingStatus() {
  return useQuery({
    queryKey: ["embedding-status"],
    queryFn: () => call(() => commands.embeddingStatus()),
    staleTime: 0,
    // Keep the coverage bar live while the worker catches up. Polls only
    // while notes remain unembedded and the backend is present, so it stops
    // on its own once coverage is full.
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data || !data.available) return false;
      return data.embedded < data.total ? 2_000 : false;
    },
  });
}

export function useSetEmbeddingModel() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (model: string) =>
      call(() => commands.embeddingSetModel(model)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["embedding-status"] });
    },
  });
}

export function useReindexEmbeddings() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => call(() => commands.embeddingReindex()),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["embedding-status"] });
    },
  });
}

/** DuckDB analytical store availability and chunk count. */
export function useAnalyticsStatus() {
  return useQuery({
    queryKey: ["analytics-status"],
    queryFn: () => call(() => commands.analyticsStatus()),
    staleTime: 0,
  });
}
