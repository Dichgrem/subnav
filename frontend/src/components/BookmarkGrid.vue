<template>
  <div v-if="groups.length === 0" class="empty-state">
    <svg class="svg-icon empty-icon" viewBox="0 0 24 24">
      <path
        d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
      ></path>
    </svg>
    {{ $t("bookmark.noBookmarks") }}
  </div>

  <div
    v-else
    v-for="(group, index) in groups"
    :key="group.categoryId ?? 'uncategorized'"
    class="bookmark-group"
    :id="'group-' + (group.categoryId ?? 'uncategorized')"
  >
    <h3 v-if="!(skipFirstTitle && index === 0)" class="group-title">
      {{ group.categoryName }}
    </h3>
    <div
      v-if="group.items.length === 0 && !group.hasChildren"
      class="group-empty"
    >
      {{ $t("bookmark.noBookmarks") }}
    </div>
    <div v-else class="book-grid">
      <div
        v-for="book in group.items"
        :key="book.id"
        class="book-card"
        :class="{ 'drag-over': dragOverId === book.id }"
        draggable="true"
        @dragstart="onDragStart($event, book)"
        @dragend="
          $emit('dragEnd');
          dragOverId = null;
        "
        @dragover.prevent="onDragOverBook($event, book)"
        @dragleave="onDragLeaveBook"
        @drop.prevent="onDropOnBook($event, book)"
      >
        <div class="book-main">
          <img
            v-if="book.icon && !failedIconSrc.has(book.icon)"
            :src="iconSrc(book.icon)"
            class="book-icon"
            loading="lazy"
            decoding="async"
            @error="onIconError(book.icon)"
          />
          <span v-else class="book-icon book-icon-placeholder">{{
            iconInitial(book)
          }}</span>
          <a :href="book.url" target="_blank" class="book-title">{{
            book.title
          }}</a>
        </div>
        <span class="book-url-text" :title="book.url">{{ book.url }}</span>
        <div class="book-actions">
          <span
            class="action-icon"
            @click="$emit('edit', book)"
            :title="$t('bookmark.edit')"
          >
            <svg class="svg-icon" viewBox="0 0 24 24">
              <path d="M12 20h9"></path>
              <path
                d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"
              ></path>
            </svg>
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref } from "vue";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const props = defineProps({
  groups: Array,
  skipFirstTitle: Boolean,
});
const emit = defineEmits(["dragStart", "dragEnd", "edit", "reorder"]);

const dragOverId = ref(null);
const failedIconSrc = ref(new Set());

// Icons are stored as base64 in the database, but the list only exposes a
// short /bookmark/icon/{id}/{hash} URL. Resolve it against the API origin in
// dev, where the frontend and backend run on different ports.
const API_ORIGIN = import.meta.env.DEV ? "http://localhost:8989" : "";
const iconSrc = (icon) =>
  icon?.startsWith("/bookmark/icon/") ? `${API_ORIGIN}${icon}` : icon;

// If an icon still fails to load (legacy external URL), fall back to a local
// letter placeholder.
const onIconError = (src) => {
  failedIconSrc.value.add(src);
};

const iconInitial = (book) => {
  const text = (book.title || book.url || "?").trim();
  return text ? text.charAt(0).toUpperCase() : "?";
};

const onDragStart = (event, book) => {
  event.dataTransfer.setData("text/plain", String(book.id));
  event.dataTransfer.effectAllowed = "move";
  emit("dragStart", event, book);
};
const onDragOverBook = (event, book) => {
  const draggedIdStr = event.dataTransfer.getData("text/plain");
  if (!draggedIdStr || parseInt(draggedIdStr, 10) === book.id) return;
  dragOverId.value = book.id;
};
const onDragLeaveBook = () => {
  dragOverId.value = null;
};
const onDropOnBook = (event, targetBook) => {
  const draggedIdStr = event.dataTransfer.getData("text/plain");
  if (!draggedIdStr) return;
  const draggedId = parseInt(draggedIdStr, 10);
  if (draggedId === targetBook.id) return;
  dragOverId.value = null;
  emit("reorder", draggedId, targetBook.id);
};
</script>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 15px;
  padding: 80px;
  color: #bbbbbb;
  font-size: 16px;
}
.empty-icon {
  width: 48px;
  height: 48px;
}
.bookmark-group {
  margin-bottom: 32px;
}
.group-title {
  font-size: 16px;
  font-weight: 600;
  color: #666;
  margin-bottom: 12px;
  padding-left: 4px;
}
.group-empty {
  color: #ccc;
  font-size: 13px;
  padding: 16px 4px;
}
.book-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 10px;
}
.book-card {
  background: white;
  border-radius: 10px;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  border: 1px solid rgba(0, 0, 0, 0.05);
  transition:
    transform 0.2s,
    box-shadow 0.2s;
  cursor: pointer;
  position: relative;
}
.book-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 16px rgba(0, 0, 0, 0.08);
}
.book-card:active {
  cursor: grabbing;
}
.book-card.drag-over {
  border-color: #dda142;
  box-shadow: 0 0 0 2px rgba(221, 161, 66, 0.3);
}
.book-main {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.book-icon {
  width: 18px;
  height: 18px;
  border-radius: 3px;
  flex-shrink: 0;
}
.book-icon-placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(128, 128, 128, 0.18);
  color: #999;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  user-select: none;
}
.book-title {
  font-weight: 500;
  color: #333333;
  text-decoration: none;
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
}
.book-title:hover {
  text-decoration: underline;
}
.book-url-text {
  font-size: 11px;
  color: #aaa;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.book-actions {
  position: absolute;
  right: 8px;
  top: 8px;
  opacity: 0;
  transition: opacity 0.2s;
}
.book-card:hover .book-actions {
  opacity: 1;
}
.action-icon {
  cursor: pointer;
  color: #ccc;
  transition: color 0.2s;
}
.action-icon:hover {
  color: #dda142;
}
.action-icon .svg-icon {
  width: 14px;
  height: 14px;
}
.svg-icon {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
</style>
