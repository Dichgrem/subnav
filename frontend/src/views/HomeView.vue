<template>
  <div class="home-app" :data-theme="darkMode ? 'dark' : 'light'">
    <div class="layout">
      <Sidebar
        v-model:collapsed="sidebarCollapsed"
        :tree="cateTree"
        :active-id="selectedCategoryId"
        :drag-hover-id="dragHoverCategoryId"
        :eating-id="eatingCategoryId"
        :dark-mode="darkMode"
        @select="scrollToGroup"
        @command="handleCommand"
        @logout="logout"
        @add-menu="handleAddMenu"
        @toggle-dark="toggleDarkMode"
        @add-sub="openAddSubCate"
        @edit-cate="openEditCate"
        @drag-start="
          (id) => {
            isDraggingCategory = true;
            draggingCateId = id;
          }
        "
        @drag-end="
          () => {
            isDraggingCategory = false;
            draggingCateId = null;
            dragHoverTrash = false;
          }
        "
        @drop="handleNodeDrop"
        @drag-over="onDragOver"
        @drag-leave="onDragLeave"
        @drop-bookmark="(e, id) => onDrop(e, id)"
      />

      <main class="content" ref="contentRef" @scroll="onContentScroll">
        <div class="header-row">
          <h2 class="header-title">{{ firstGroupTitle }}</h2>
          <SearchBar v-model="searchQuery" />
        </div>
        <BookmarkGrid
          :groups="groupedBookmarks"
          :skip-first-title="true"
          @edit="openEditBook"
          @drag-start="onDragStart"
          @drag-end="onDragEnd"
          @reorder="onReorderBook"
        />
      </main>
    </div>

    <EditDialog
      v-model:visible="showEditBook"
      :book="editBook"
      :categories="cateList"
      @save="saveEditBook"
    />

    <CheckLinks v-model:visible="showCheckLinks" @edit="handleCheckEdit" />

    <el-dialog
      v-model="showAddSubCate"
      :title="$t('category.addSub')"
      width="400px"
    >
      <input
        v-model="newSubCateName"
        class="modern-input full-width"
        :placeholder="$t('category.name')"
        @keyup.enter="saveAddSubCate"
      />
      <template #footer>
        <DialogFooter @cancel="showAddSubCate = false" @save="saveAddSubCate" />
      </template>
    </el-dialog>

    <el-dialog
      v-model="showEditCate"
      :title="$t('category.edit')"
      width="400px"
    >
      <input v-model="editCate.name" class="modern-input full-width" />
      <template #footer>
        <DialogFooter @cancel="showEditCate = false" @save="saveEditCate" />
      </template>
    </el-dialog>

    <el-dialog
      v-model="showChangePwd"
      :title="$t('sidebar.changePassword')"
      width="400px"
    >
      <input
        v-model="changePwdForm.oldPassword"
        type="password"
        class="modern-input full-width"
        :placeholder="$t('userAdmin.oldPassword')"
        style="margin-bottom: 12px"
      />
      <input
        v-model="changePwdForm.newPassword"
        type="password"
        class="modern-input full-width"
        :placeholder="$t('userAdmin.newPassword')"
      />
      <template #footer>
        <DialogFooter @cancel="showChangePwd = false" @save="handleChangePwd" />
      </template>
    </el-dialog>

    <!-- trash container -->
    <div
      class="trash-container"
      :class="{
        'dragging-active': isDraggingAny,
        'drag-over': dragHoverTrash,
        eating: eatingTrash,
      }"
      @dragover.prevent="dragHoverTrash = true"
      @dragleave="dragHoverTrash = false"
      @drop.prevent="onTrashDrop"
    >
      <div class="trash-wrapper">
        <svg
          class="pixel-trash"
          :class="{ hide: dragHoverTrash || eatingTrash }"
          viewBox="0 0 13 13"
          shape-rendering="crispEdges"
        >
          <rect x="4" y="2" width="5" height="1" fill="#bbbbbb" />
          <rect x="2" y="3" width="9" height="1" fill="#bbbbbb" />
          <rect x="3" y="4" width="1" height="7" fill="#bbbbbb" />
          <rect x="9" y="4" width="1" height="7" fill="#bbbbbb" />
          <rect x="3" y="11" width="7" height="1" fill="#bbbbbb" />
          <rect x="5" y="5" width="1" height="5" fill="#bbbbbb" />
          <rect x="7" y="5" width="1" height="5" fill="#bbbbbb" />
        </svg>
        <PixelPacman
          class="pixel-pacman flipped"
          :class="{ show: dragHoverTrash || eatingTrash }"
        />
      </div>
      <div class="undo-bar" v-if="pendingDeletes.length">
        <button
          v-for="d in pendingDeletes.filter((p) => p.timer !== null)"
          :key="d.id"
          class="undo-btn"
          @click="undoDelete(d)"
          :title="
            $t('bookmark.undoDelete') +
            ' ' +
            (d.item?.name || d.item?.title || '')
          "
        >
          <svg viewBox="0 0 24 24" class="undo-icon">
            <path d="M1 4v6h6"></path>
            <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"></path>
          </svg>
        </button>
      </div>
    </div>

    <input
      type="file"
      ref="fileInput"
      style="display: none"
      @change="handleFileUpload"
    />

    <!-- fullscreen animation overlay -->
    <div class="fullscreen-overlay" v-if="isImportingAnim || isExportingAnim">
      <div class="anim-theater">
        <PixelPacman
          class="pixel-pacman giant-pacman"
          :class="{
            spitting: isExportingAnim,
            'eating-frenzy': isImportingAnim,
          }"
        />
        <div class="particles import-particles" v-if="isImportingAnim">
          <div
            class="particle"
            v-for="i in 10"
            :key="'in' + i"
            :style="{ animationDelay: i * 0.1 + 's' }"
          ></div>
        </div>
        <div class="particles export-particles" v-if="isExportingAnim">
          <div
            class="particle"
            v-for="i in 15"
            :key="'out' + i"
            :style="{
              '--tx': Math.random() * 300 - 150 + 'px',
              '--ty': Math.random() * 200 - 100 + 'px',
              animationDelay: i * 0.05 + 's',
            }"
          ></div>
        </div>
      </div>
      <div class="anim-text">
        {{
          isImportingAnim ? $t("bookmark.importing") : $t("bookmark.exporting")
        }}
      </div>
    </div>

    <button
      class="back-to-top"
      :class="{ visible: showBackToTop }"
      @click="scrollToTop"
      :title="$t('app.backToTop')"
    >
      <svg viewBox="0 0 24 24" width="18" height="18">
        <polyline
          points="18 15 12 9 6 15"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        ></polyline>
      </svg>
    </button>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import request from "../utils/request.js";
import {
  LS_TOKEN,
  LS_USER,
  LS_DARK,
  LS_LOCALE,
  LS_EXPANDED,
  LS_ICON_MIGRATED,
} from "../utils/constants.js";
import { buildTree } from "../utils/tree.js";
import { useBookmarks, isExternalIcon } from "../composables/useBookmarks.js";
import { useDragDrop } from "../composables/useDragDrop.js";
import { useScroll } from "../composables/useScroll.js";
import { useImportExport } from "../composables/useImportExport.js";
import { useChangePassword } from "../composables/useChangePassword.js";
import Sidebar from "../components/Sidebar.vue";
import SearchBar from "../components/SearchBar.vue";
import BookmarkGrid from "../components/BookmarkGrid.vue";
import EditDialog from "../components/EditDialog.vue";
import CheckLinks from "../components/CheckLinks.vue";
import PixelPacman from "../components/PixelPacman.vue";
import DialogFooter from "../components/DialogFooter.vue";
import Fuse from "fuse.js";
let _pinyin = null;
import("pinyin-pro").then((m) => {
  _pinyin = m.pinyin;
});

const { t } = useI18n();
const router = useRouter();

const {
  bookList,
  cateList,
  loadData,
  showEditBook,
  editBook,
  openEditBook,
  saveEditBook,
  showEditCate,
  editCate,
  openEditCate,
  saveEditCate,
  addCate,
  showAddSubCate,
  newSubCateName,
  openAddSubCate,
  saveAddSubCate,
  fetchIcons,
  handleAddMenu,
} = useBookmarks();

const darkMode = ref(localStorage.getItem(LS_DARK) === "true");

const selectedCategoryId = ref(null);
const sidebarCollapsed = ref(false);
const searchQuery = ref("");

const {
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
} = useDragDrop(bookList, cateList, selectedCategoryId, loadData);

const visibleBookList = computed(() =>
  bookList.value.filter((b) => !pendingBookIds.value.has(b.id)),
);
const visibleCateList = computed(() =>
  cateList.value.filter((c) => !pendingCateIds.value.has(c.id)),
);

const cateTree = computed(() => buildTree(visibleCateList.value));

const contentRef = ref(null);
const { showBackToTop, onContentScroll, scrollToTop, scrollToGroup } =
  useScroll(contentRef, selectedCategoryId);

const handleNodeDrop = async () => {
  const flatUpdates = [];
  let order = 0;
  const flatten = (nodes, parentId = null) => {
    nodes.forEach((n) => {
      flatUpdates.push({
        id: n.id,
        name: n.name,
        parentId,
        sortOrder: order++,
      });
      if (n.children) flatten(n.children, n.id);
    });
  };
  flatten(cateTree.value);
  try {
    await request.post("/category/batchUpdate", flatUpdates);
  } catch {
    ElMessage.error(t("bookmark.sortFailed"));
    loadData();
  }
};

const showCheckLinks = ref(false);

const {
  fileInput,
  isImportingAnim,
  isExportingAnim,
  exportBookmarks,
  triggerImport,
  handleFileUpload,
} = useImportExport(loadData);

const { showChangePwd, changePwdForm, handleChangePwd } = useChangePassword();

const handleCommand = (command) => {
  if (command === "export-html") exportBookmarks();
  else if (command === "import-html") triggerImport();
  else if (command === "fetch-icons") fetchIcons();
  else if (command === "lang-zh") switchLang("zh-CN");
  else if (command === "lang-en") switchLang("en-US");
  else if (command === "change-password") showChangePwd.value = true;
  else if (command === "check-links") showCheckLinks.value = true;
};

const handleCheckEdit = (bookmarkId) => {
  const book = bookList.value.find((b) => b.id === bookmarkId);
  if (book) openEditBook(book);
};

const { locale } = useI18n();
const switchLang = (lang) => {
  locale.value = lang;
  localStorage.setItem(LS_LOCALE, lang);
};

function expandKeywords(bookmark) {
  const t = bookmark.title || "";
  const u = (bookmark.url || "").toLowerCase();
  const words = [
    t,
    t.toLowerCase(),
    t.replace(/\s+/g, ""),
    (t.match(/[A-Z]/g) || []).join("").toLowerCase(),
    u,
  ];
  if (_pinyin) {
    words.push(
      _pinyin(t, { toneType: "none", type: "array" }).join(""),
      _pinyin(t, { toneType: "none", type: "array", pattern: "first" }).join(
        "",
      ),
    );
  }
  return words.join(" ");
}

const fuseIndex = computed(
  () =>
    new Fuse(visibleBookList.value, {
      keys: [{ name: "title", getFn: expandKeywords }],
      threshold: 0.4,
      minMatchCharLength: 1,
    }),
);

const filteredBookList = computed(() => {
  const q = searchQuery.value.trim();
  if (!q) return visibleBookList.value;
  return fuseIndex.value.search(q).map((r) => r.item);
});

const groupedBookmarks = computed(() => {
  const groups = {};
  const categoryNames = {};
  const catMap = {};
  visibleCateList.value.forEach((c) => {
    categoryNames[c.id] = c.name;
    catMap[c.id] = c;
  });

  const computeDepth = (id, d) => {
    if (id == null) return d;
    const p = catMap[id];
    if (!p || p.parentId == null) return d;
    return computeDepth(p.parentId, d + 1);
  };
  const hasChildren = {};
  visibleCateList.value.forEach((c) => {
    if (c.parentId != null) hasChildren[c.parentId] = true;
  });

  filteredBookList.value.forEach((book) => {
    const cid = book.categoryId ?? null;
    if (!groups[cid]) {
      groups[cid] = {
        categoryId: cid,
        categoryName: categoryNames[cid] || t("bookmark.uncategorized"),
        depth: cid ? computeDepth(cid, 0) : 0,
        hasChildren: cid ? !!hasChildren[cid] : false,
        items: [],
      };
    }
    groups[cid].items.push(book);
  });

  if (!searchQuery.value.trim()) {
    visibleCateList.value.forEach((c) => {
      if (!groups[c.id]) {
        groups[c.id] = {
          categoryId: c.id,
          categoryName: c.name,
          depth: computeDepth(c.id, 0),
          hasChildren: !!hasChildren[c.id],
          items: [],
        };
      }
    });
  }

  const order = [null, ...visibleCateList.value.map((c) => c.id)];
  return order
    .filter((cid) => groups[cid])
    .map((cid) => {
      const g = groups[cid];
      g.items.sort((a, b) => (a.sortOrder || 0) - (b.sortOrder || 0));
      return g;
    });
});

const firstGroupTitle = computed(
  () => groupedBookmarks.value[0]?.categoryName || "",
);

const logout = () => {
  pendingDeletes.value.forEach((d) => clearTimeout(d.timer));
  pendingDeletes.value = [];
  localStorage.removeItem(LS_TOKEN);
  localStorage.removeItem(LS_USER);
  localStorage.removeItem(LS_EXPANDED);
  ElMessage.success(t("app.logoutSuccess"));
  router.push({ name: "Login" });
};
const toggleDarkMode = () => {
  darkMode.value = !darkMode.value;
  localStorage.setItem(LS_DARK, darkMode.value);
  document.documentElement.classList.toggle("dark", darkMode.value);
  document.body.classList.toggle("dark", darkMode.value);
};

onMounted(async () => {
  if (darkMode.value) {
    document.documentElement.classList.add("dark");
    document.body.classList.add("dark");
  }
  await loadData();

  // One-time migration: turn legacy external favicon URLs into local data URIs.
  if (
    !localStorage.getItem(LS_ICON_MIGRATED) &&
    bookList.value.some((book) => isExternalIcon(book.icon))
  ) {
    localStorage.setItem(LS_ICON_MIGRATED, "1");
    fetchIcons(undefined, true);
  }
});
</script>

<style scoped>
.home-app {
  margin: 0;
  font-family:
    "Inter",
    -apple-system,
    BlinkMacSystemFont,
    "Segoe UI",
    Roboto,
    sans-serif;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background-color: #f5f5f5;
  color: #222222;
}
.layout {
  flex: 1;
  display: flex;
  overflow: hidden;
}
.content {
  flex: 1;
  padding: 30px;
  overflow-y: auto;
  background-color: #fbfbfb;
}

.header-row {
  display: flex;
  align-items: center;
  position: relative;
  margin-bottom: 24px;
}
.header-title {
  font-size: 20px;
  font-weight: 600;
  color: #333;
  flex-shrink: 0;
}
.header-row :deep(.search-bar) {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  width: 480px;
  max-width: calc(100% - 220px);
}

.modern-input {
  padding: 10px 14px;
  border: 1px solid #e0e0e0;
  border-radius: 8px;
  font-size: 14px;
  outline: none;
  transition: border-color 0.2s;
  background: white;
  color: #333;
}
.modern-input:focus {
  border-color: #dda142;
  box-shadow: 0 0 0 3px rgba(221, 161, 66, 0.1);
}
.full-width {
  width: 100%;
  box-sizing: border-box;
}
.primary-btn {
  background-color: #333;
  color: #fff;
  border: none;
  padding: 12px 25px;
  border-radius: 30px;
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;
}
.primary-btn:hover {
  background-color: #111;
  transform: translateY(-1px);
}
.outline-btn {
  background: transparent;
  border: 1px solid #333;
  color: #333;
  padding: 10px 20px;
  border-radius: 30px;
  cursor: pointer;
  font-size: 13px;
  transition: all 0.2s;
}
.outline-btn:hover {
  background-color: #f5f5f5;
}
.small-btn {
  padding: 10px 15px;
  font-size: 13px;
}
.outline-btn.small-btn {
  border-color: #333;
  color: #333;
}
.outline-btn.small-btn:hover {
  background-color: #f5f5f5;
}
:deep(.el-dialog__footer) {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.trash-container {
  position: fixed;
  right: 30px;
  bottom: 30px;
  width: 75px;
  height: 75px;
  background-color: white;
  border-radius: 50%;
  box-shadow: 0 4px 15px rgba(0, 0, 0, 0.1);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  opacity: 0.5;
  z-index: 100;
}
.trash-container.dragging-active {
  opacity: 0.85;
  transform: scale(1.05);
}
.trash-container.drag-over {
  opacity: 1;
  box-shadow: 0 6px 24px rgba(221, 161, 66, 0.3);
  background-color: #fff8f0;
}
.trash-container.eating {
  opacity: 1;
  transform: scale(0.9);
  background-color: #fff8f0;
}

.trash-wrapper {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}
.pixel-trash {
  width: 44px;
  height: 44px;
  position: absolute;
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  transform-origin: center bottom;
}
.pixel-trash.hide {
  opacity: 0;
  transform: scale(0.2);
}
.trash-wrapper .pixel-pacman {
  width: 44px;
  height: 44px;
  position: absolute;
  opacity: 0;
  transform: scale(0.2) scaleX(-1);
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.trash-wrapper .pixel-pacman.show {
  opacity: 1;
  transform: scale(1.3) scaleX(-1);
}

@keyframes pounce-left {
  0% {
    opacity: 1;
    transform: scale(1.3) scaleX(-1);
  }
  50% {
    transform: scale(1.5) translateX(-8px) scaleX(-1);
  }
  75% {
    transform: scale(1.5) translateX(-8px) scaleX(-1);
    opacity: 1;
  }
  100% {
    transform: scale(1.1) translateX(0) scaleX(-1);
    opacity: 1;
  }
}
.eating .trash-wrapper .pixel-pacman.show {
  animation: pounce-left 0.6s ease-in-out forwards;
}
.eating .pixel-trash {
  opacity: 0;
  transform: scale(0);
}
.eating .pac-jaw-top,
.eating .pac-jaw-bottom,
.drag-over .pac-jaw-top,
.drag-over .pac-jaw-bottom {
  animation: chomp-trash 0.12s infinite alternate;
}
.eating .pac-jaw-mid,
.drag-over .pac-jaw-mid {
  animation: chomp-trash-mid 0.12s infinite alternate;
}
@keyframes chomp-trash {
  0% {
    opacity: 0;
  }
  100% {
    opacity: 1;
  }
}
@keyframes chomp-trash-mid {
  0% {
    opacity: 0;
    fill: #dda142;
  }
  100% {
    opacity: 1;
    fill: #222;
  }
}
@keyframes chomp-jaw {
  0%,
  30% {
    opacity: 0;
  }
  70%,
  100% {
    opacity: 1;
  }
}
@keyframes chomp-mid {
  0%,
  40% {
    opacity: 0;
  }
  60%,
  100% {
    opacity: 1;
  }
}

.back-to-top {
  position: fixed;
  bottom: 117px;
  right: 47px;
  width: 42px;
  height: 42px;
  background: white;
  color: #dda142;
  border: none;
  border-radius: 50%;
  cursor: pointer;
  opacity: 0;
  visibility: hidden;
  transform: translateY(8px);
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 99;
  box-shadow: 0 3px 12px rgba(0, 0, 0, 0.12);
}
.back-to-top.visible {
  opacity: 1;
  visibility: visible;
  transform: translateY(0);
}
.back-to-top:hover {
  background: #dda142;
  color: white;
  box-shadow: 0 6px 20px rgba(221, 161, 66, 0.3);
}

.fullscreen-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background-color: rgba(0, 0, 0, 0.85);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}
.giant-pacman {
  width: 160px;
  height: 160px;
  transform-origin: center;
}
.anim-theater {
  position: relative;
  width: 300px;
  height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.giant-pacman.eating-frenzy .pac-jaw-top,
.giant-pacman.eating-frenzy .pac-jaw-bottom {
  animation: frenzy-chomp-jaw 0.1s infinite alternate;
}
.giant-pacman.eating-frenzy .pac-jaw-mid {
  animation: frenzy-chomp-mid 0.1s infinite alternate;
}
@keyframes frenzy-chomp-jaw {
  0% {
    opacity: 0;
  }
  100% {
    opacity: 1;
  }
}
@keyframes frenzy-chomp-mid {
  0% {
    opacity: 0;
    fill: #dda142;
  }
  100% {
    opacity: 1;
    fill: #222;
  }
}
.giant-pacman.spitting {
  transform: scaleX(-1);
  animation: spit-recoil 0.5s ease-in-out infinite;
}
.giant-pacman.spitting .pac-jaw-top,
.giant-pacman.spitting .pac-jaw-bottom {
  opacity: 1;
}
.giant-pacman.spitting .pac-jaw-mid {
  opacity: 1;
  fill: #222;
}
@keyframes spit-recoil {
  0%,
  100% {
    transform: scaleX(-1) translateX(0);
  }
  50% {
    transform: scaleX(-1) translateX(-10px);
  }
}
.anim-text {
  margin-top: 30px;
  color: #dda142;
  font-size: 24px;
  font-weight: 600;
  letter-spacing: 2px;
}
.particles {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.particle {
  position: absolute;
  width: 12px;
  height: 12px;
  background-color: white;
  border-radius: 2px;
  opacity: 0;
}
.import-particles .particle {
  top: 50%;
  right: -50vw;
  transform: translateY(-50%);
  animation: fly-in 1s linear infinite;
  background-color: #dda142;
}
@keyframes fly-in {
  0% {
    right: -50vw;
    opacity: 1;
    transform: translateY(-50%) scale(1);
  }
  90% {
    right: 80px;
    opacity: 1;
    transform: translateY(-50%) scale(0.5);
  }
  100% {
    right: 120px;
    opacity: 0;
    transform: translateY(-50%) scale(0);
  }
}
.export-particles .particle {
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  animation: spray-out 0.8s cubic-bezier(0.25, 1, 0.5, 1) infinite;
  background-color: #dda142;
}
@keyframes spray-out {
  0% {
    transform: translate(-20px, 0) scale(0);
    opacity: 1;
  }
  80% {
    opacity: 1;
  }
  100% {
    transform: translate(calc(-50px + var(--tx)), var(--ty)) scale(1.5);
    opacity: 0;
  }
}
.undo-bar {
  position: absolute;
  right: calc(100% + 12px);
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  gap: 8px;
  align-items: center;
}
.undo-btn {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: #333;
  color: #fff;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background 0.2s,
    transform 0.2s;
  animation: undo-pop 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.undo-btn:hover {
  background: #555;
  transform: scale(1.1);
}
@keyframes undo-pop {
  from {
    opacity: 0;
    transform: translateX(10px) scale(0.5);
  }
  to {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
}
.undo-icon {
  width: 18px;
  height: 18px;
  stroke: #fff;
  stroke-width: 2;
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
}

[data-theme="dark"] .home-app {
  background-color: #1e1e1e;
  color: #ddd;
}
[data-theme="dark"] .content {
  background-color: #252525;
}
[data-theme="dark"] .header-row {
  background-color: transparent;
  color: #ddd;
}
[data-theme="dark"] .header-title {
  color: #ddd;
}
[data-theme="dark"] .modern-input {
  background: #363636;
  color: #ddd;
  border-color: #555;
}
[data-theme="dark"] .modern-input:focus {
  border-color: #dda142;
}
[data-theme="dark"] .back-to-top {
  background: #363636;
  color: #dda142;
  box-shadow: 0 3px 12px rgba(0, 0, 0, 0.3);
}
[data-theme="dark"] .back-to-top:hover {
  background: #dda142;
  color: #1e1e1e;
}
[data-theme="dark"] .dialog-label {
  color: #aaa;
}
[data-theme="dark"] select.modern-input {
  background: #363636;
  color: #ddd;
}
[data-theme="dark"] select.modern-input option {
  background: #363636;
  color: #ddd;
}
[data-theme="dark"] .anim-text {
  color: #dda142;
}
[data-theme="dark"] .group-empty {
  color: #666;
}
</style>

<style>
html.dark .el-dialog {
  background-color: #2d2d2d;
}
html.dark .el-dialog__title {
  color: #ddd;
}
html.dark .el-dialog__headerbtn .el-dialog__close {
  color: #999;
}
html.dark .el-dialog__headerbtn .el-dialog__close:hover {
  color: #dda142;
}
html.dark .el-dropdown-menu {
  background-color: #2d2d2d;
}
html.dark .el-dropdown-menu__item {
  color: #ccc;
}
html.dark .el-dropdown-menu__item:hover {
  background-color: #3a3a3a;
}
html.dark .el-dropdown-menu__item--divided {
  border-top-color: #444;
}
html.dark .el-message {
  background-color: #2d2d2d;
  border-color: #555;
}
html.dark .el-message .el-message__content {
  color: #ddd;
}
html.dark .el-tree {
  background-color: transparent;
}
html.dark .el-tree-node__content {
  background-color: transparent !important;
}
html.dark .el-tree-node__content:hover {
  background-color: rgba(255, 255, 255, 0.06) !important;
}
html.dark .el-tree-node:focus > .el-tree-node__content {
  background-color: transparent !important;
}
html.dark .el-popper.is-dark {
  background: #2d2d2d;
  color: #ddd;
}
html.dark .book-card {
  background: #2d2d2d;
  color: #ddd;
  border-color: #444;
}
html.dark .book-card:hover {
  box-shadow: 0 6px 16px rgba(0, 0, 0, 0.3);
}
html.dark .book-title {
  color: #ddd;
}
html.dark .book-url-text {
  color: #999;
}
html.dark .group-title {
  color: #bbb;
}
html.dark .group-empty,
html.dark .empty-state {
  color: #777;
}
html.dark .search-input {
  background: #363636;
  color: #ddd;
  border-color: #555;
}
html.dark .search-input::placeholder {
  color: #888;
}
html.dark .search-icon {
  color: #888;
}
</style>
