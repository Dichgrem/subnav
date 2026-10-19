import { ref, computed, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import request from "../utils/request.js";
import { isLocalIconUrl } from "./useBookmarks.js";

export function useDragDrop(bookList, cateList, selectedCategoryId, loadData) {
  const { t } = useI18n();

  const PENDING_DELAY = 3000;
  const EAT_ANIM_MS = 600;

  const isDraggingBookmark = ref(false);
  const isDraggingCategory = ref(false);
  const draggingCateId = ref(null);
  const isDraggingAny = computed(
    () => isDraggingBookmark.value || isDraggingCategory.value,
  );
  const dragHoverCategoryId = ref(null);
  const eatingCategoryId = ref(null);
  const dragHoverTrash = ref(false);
  const eatingTrash = ref(false);
  const pendingDeletes = ref([]);

  const pendingBookIds = computed(() => {
    const s = new Set();
    pendingDeletes.value.forEach((d) => {
      if (d.type === "bookmark") s.add(d.id);
    });
    return s;
  });
  const pendingCateIds = computed(() => {
    const s = new Set();
    pendingDeletes.value.forEach((d) => {
      if (d.type === "category") s.add(d.id);
    });
    return s;
  });

  const onDragStart = (event, book) => {
    event.dataTransfer.setData("text/plain", String(book.id));
    event.dataTransfer.effectAllowed = "move";
    isDraggingBookmark.value = true;
  };

  const onDragEnd = () => {
    dragHoverCategoryId.value = null;
    dragHoverTrash.value = false;
    isDraggingBookmark.value = false;
  };

  const onReorderBook = async (draggedId, targetId) => {
    const a = bookList.value.find((b) => b.id === draggedId);
    const b = bookList.value.find((b) => b.id === targetId);
    if (!a || !b) return;
    const soa = a.sortOrder || 0;
    const sob = b.sortOrder || 0;
    a.sortOrder = sob;
    b.sortOrder = soa;
    // shallowRef: replace the array to notify dependents of nested mutations.
    bookList.value = [...bookList.value];
    await nextTick();
    try {
      await request.post("/bookmark/batchUpdateSort", [
        { id: a.id, sortOrder: a.sortOrder },
        { id: b.id, sortOrder: b.sortOrder },
      ]);
    } catch {
      a.sortOrder = soa;
      b.sortOrder = sob;
      bookList.value = [...bookList.value];
      ElMessage.error(t("bookmark.sortFailed"));
    }
  };

  const onDragOver = (cateId) => {
    if (isDraggingBookmark.value) dragHoverCategoryId.value = cateId;
  };

  const onDragLeave = () => {
    dragHoverCategoryId.value = null;
  };

  const moveBook = async (bookId, categoryId) => {
    const book = bookList.value.find((b) => b.id === bookId);
    if (book) {
      eatingCategoryId.value = categoryId;
      isDraggingBookmark.value = false;
      bookList.value = bookList.value.filter((b) => b.id !== bookId);
      setTimeout(async () => {
        eatingCategoryId.value = null;
        try {
          await request.post("/bookmark/add", {
            id: bookId,
            title: book.title,
            url: book.url,
            icon: isLocalIconUrl(book.icon) ? null : book.icon,
            categoryId,
            sortOrder: book.sortOrder || 0,
          });
          loadData();
        } catch {
          ElMessage.error(t("bookmark.moveFailed"));
          loadData();
        }
      }, EAT_ANIM_MS);
    }
  };

  const onDrop = (event, toCateId) => {
    const bookIdStr = event.dataTransfer.getData("text/plain");
    if (!bookIdStr) return;
    dragHoverCategoryId.value = null;
    dragHoverTrash.value = false;
    const bookId = parseInt(bookIdStr, 10);
    moveBook(bookId, toCateId);
  };

  const collectSubTreeIds = (cateId) => {
    const ids = [cateId];
    const q = [cateId];
    while (q.length) {
      const pid = q.shift();
      cateList.value.forEach((c) => {
        if (c.parentId === pid) {
          ids.push(c.id);
          q.push(c.id);
        }
      });
    }
    return ids;
  };

  const onTrashDrop = async (event) => {
    if (isDraggingCategory.value) {
      const cateId = draggingCateId.value;
      if (!cateId) return;
      const deleted = cateList.value.find((c) => c.id === cateId);
      if (!deleted) return;
      eatingTrash.value = true;

      const subCateIds = collectSubTreeIds(cateId);
      const subCateIdSet = new Set(subCateIds);
      const subBookIds = bookList.value
        .filter((b) => subCateIdSet.has(b.categoryId))
        .map((b) => b.id);
      const allIds = [...subCateIds, ...subBookIds];

      const timer = setTimeout(async () => {
        try {
          await request.post("/category/delete", { id: cateId });
          if (selectedCategoryId.value === cateId)
            selectedCategoryId.value = null;
        } catch {
          ElMessage.error(t("bookmark.deleteFailed"));
        }
        eatingTrash.value = false;
        pendingDeletes.value = pendingDeletes.value.filter(
          (d) => !allIds.includes(d.id),
        );
        loadData();
      }, PENDING_DELAY);

      await nextTick();
      const entry = {
        type: "category",
        id: cateId,
        idList: allIds,
        timer,
        item: { ...deleted },
      };
      pendingDeletes.value.push(entry);
      subCateIds.slice(1).forEach((sid) => {
        pendingDeletes.value.push({
          type: "category",
          id: sid,
          idList: allIds,
          timer: null,
          item: null,
        });
      });
      subBookIds.forEach((bid) => {
        pendingDeletes.value.push({
          type: "bookmark",
          id: bid,
          idList: allIds,
          timer: null,
          item: null,
        });
      });

      setTimeout(() => {
        eatingTrash.value = false;
      }, EAT_ANIM_MS);
      return;
    }
    if (isDraggingBookmark.value) {
      const bookIdStr = event.dataTransfer.getData("text/plain");
      if (!bookIdStr) return;
      const bookId = parseInt(bookIdStr, 10);
      const deleted = bookList.value.find((b) => b.id === bookId);
      if (!deleted) return;
      eatingTrash.value = true;
      const timer = setTimeout(async () => {
        try {
          await request.post("/bookmark/delete", { id: bookId });
        } catch {
          ElMessage.error(t("bookmark.deleteFailed"));
        }
        eatingTrash.value = false;
        pendingDeletes.value = pendingDeletes.value.filter(
          (d) => d.id !== bookId,
        );
        loadData();
      }, PENDING_DELAY);
      await nextTick();
      pendingDeletes.value.push({
        type: "bookmark",
        id: bookId,
        timer,
        item: { ...deleted },
      });
      setTimeout(() => {
        eatingTrash.value = false;
      }, EAT_ANIM_MS);
    }
  };

  const undoDelete = (entry) => {
    clearTimeout(entry.timer);
    const ids = entry.idList || [entry.id];
    const idSet = new Set(ids);
    pendingDeletes.value = pendingDeletes.value.filter((d) => !idSet.has(d.id));
  };

  return {
    isDraggingBookmark,
    isDraggingCategory,
    draggingCateId,
    isDraggingAny,
    dragHoverCategoryId,
    eatingCategoryId,
    dragHoverTrash,
    eatingTrash,
    pendingDeletes,
    pendingBookIds,
    pendingCateIds,
    onDragStart,
    onDragEnd,
    onReorderBook,
    onDragOver,
    onDragLeave,
    onDrop,
    onTrashDrop,
    undoDelete,
  };
}
