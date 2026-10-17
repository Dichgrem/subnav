import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import request from "../utils/request.js";

export const isExternalIcon = (icon) => /^https?:\/\//i.test(icon || "");

const needsLocalIcon = (icon) => !icon || isExternalIcon(icon);

export function useBookmarks() {
  const { t } = useI18n();

  const bookList = ref([]);
  const cateList = ref([]);

  const showEditBook = ref(false);
  const editBook = ref({});

  const showEditCate = ref(false);
  const editCate = ref({});

  const showAddSubCate = ref(false);
  const addSubParentId = ref(null);
  const newSubCateName = ref("");

  const loadData = async () => {
    try {
      const [c, b] = await Promise.all([
        request.get("/category/list"),
        request.get("/bookmark/list"),
      ]);
      cateList.value = c.data.data || [];
      bookList.value = b.data.data || [];
    } catch {
      ElMessage.error(t("bookmark.loadFailed"));
    }
  };

  const openEditBook = (book) => {
    editBook.value = { ...book };
    showEditBook.value = true;
  };

  const saveEditBook = async (form) => {
    try {
      const res = await request.post("/bookmark/add", { ...form });
      showEditBook.value = false;
      const saved = res.data?.data;
      await loadData();
      // Localize the icon in the background so saving stays fast.
      if (saved?.id && needsLocalIcon(saved.icon)) {
        fetchIcons([saved.id], true);
      }
    } catch {
      ElMessage.error(t("bookmark.saveFailed"));
    }
  };

  const addCate = (parentId) => {
    editCate.value = { parentId, name: "", sortOrder: 0 };
    showEditCate.value = true;
  };

  const openEditCate = (data) => {
    editCate.value = { ...data };
    showEditCate.value = true;
  };

  const saveEditCate = async () => {
    try {
      await request.post("/category/add", editCate.value);
      showEditCate.value = false;
      loadData();
    } catch {
      ElMessage.error(t("bookmark.saveFailed"));
    }
  };

  const openAddSubCate = (parentId) => {
    addSubParentId.value = parentId;
    newSubCateName.value = "";
    showAddSubCate.value = true;
  };

  const saveAddSubCate = async () => {
    if (!newSubCateName.value.trim()) {
      ElMessage.warning(t("category.nameRequired"));
      return;
    }
    try {
      await request.post("/category/add", {
        name: newSubCateName.value,
        parentId: addSubParentId.value,
        sortOrder: 0,
      });
      showAddSubCate.value = false;
      loadData();
    } catch {
      ElMessage.error(t("bookmark.saveFailed"));
    }
  };

  const fetchIcons = async (ids, silent = false) => {
    const loading = silent
      ? null
      : ElMessage({ message: t("bookmark.fetchingIcons"), duration: 0 });
    try {
      const res = await request.post(
        "/bookmark/fetchIcons",
        ids?.length ? { ids } : {},
        { timeout: 120000 },
      );
      if (res.data.code !== 200) {
        if (!silent) {
          ElMessage.error(res.data.msg || t("bookmark.fetchIconFailed"));
        }
        return null;
      }

      const result = res.data.data || {};
      await loadData();

      if (!silent) {
        if (result.updated > 0) {
          ElMessage.success(
            t("bookmark.fetchIconSuccess", { count: result.updated }),
          );
        } else if (result.total === 0) {
          ElMessage.info(t("bookmark.fetchIconNone"));
        } else {
          ElMessage.warning(t("bookmark.fetchIconFailed"));
        }
      }
      return result;
    } catch {
      if (!silent) ElMessage.error(t("bookmark.fetchIconFailed"));
      return null;
    } finally {
      loading?.close();
    }
  };

  const handleAddMenu = (type) => {
    if (type === "bookmark") {
      editBook.value = {};
      showEditBook.value = true;
    } else {
      addCate(null);
    }
  };

  return {
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
    addSubParentId,
    newSubCateName,
    openAddSubCate,
    saveAddSubCate,
    fetchIcons,
    handleAddMenu,
  };
}
