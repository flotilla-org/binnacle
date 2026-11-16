// How a value reads where it is named: a group's label, a Distinct Value, a
// chip. A null is not an empty string, and the two read apart.

/** A null: drawn dim, as a null cell is. */
export const NULL_LABEL = "(null)";

/** An empty string. */
export const EMPTY_LABEL = "(empty)";

/** `value` as it reads where it is named: itself, or one of the two labels for a null and an empty string. */
export const valueLabel = (value: string | null): string => (value === null ? NULL_LABEL : value === "" ? EMPTY_LABEL : value);
