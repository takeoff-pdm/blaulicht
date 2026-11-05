"""EDMFormer's dataset, plus frames without a label: NO_LABEL (the rule's Groove, gaps in hand labels) stays
out of the label loss. The upstream class knows that mask (`function_mask`) but only sets it in its adapters."""

from dataset.label2id import LABEL_TO_ID
from dataset.SongFormerDataset import Dataset as Upstream

NO_LABEL = LABEL_TO_ID["NO_LABEL"]


class Dataset(Upstream):
    def __getitem__(self, idx):
        item = super().__getitem__(idx)
        if item is not None:
            item["function_mask"] = item["true_function"][:, NO_LABEL] > 0
        return item
