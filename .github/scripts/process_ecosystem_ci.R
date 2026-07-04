# Summarizes the difference between the reference Air binary's formatted output
# and the current project's, across a set of repositories.
#
# The workflow formats each repo with both tools in both orders (see the
# workflow comment). For each repo it produces:
#   <repo>_current_vs_air.diff  -> baseline Air, then current: where the current
#                                  project changes Air's output
#   <repo>_air_vs_current.diff  -> baseline current, then Air: where Air changes
#                                  the current project's output
#   <repo>_air_stderr.txt       -> Air's parse/format failures on the original
#   <repo>_current_stderr.txt   -> the current project's failures on the original
#
# Either diff being non-empty is a place the two formatters produce different
# output for the same code -- something `--check` cannot reveal.

repos_raw <- Sys.getenv("TEST_REPOS")
repo_lines <- strsplit(repos_raw, "\n")[[1]]
repo_lines <- trimws(repo_lines)
repo_lines <- repo_lines[repo_lines != ""]

# Parse the repo list. Comment lines (e.g. "# packages", "# other") act as
# category markers: every repo listed below such a line belongs to that
# category, which is later used to group results under a subheader.
repo_names <- character(0)
repo_shas <- character(0)
repo_categories <- character(0)
current_category <- NA_character_

for (line in repo_lines) {
  if (startsWith(line, "#")) {
    marker <- trimws(sub("^#+", "", line))
    current_category <- if (grepl("package", marker, ignore.case = TRUE)) {
      "Packages"
    } else {
      "Other repos"
    }
    next
  }
  parts <- strsplit(line, "@")[[1]]
  repo_names <- c(repo_names, trimws(parts[1]))
  repo_shas <- c(repo_shas, trimws(parts[2]))
  repo_categories <- c(repo_categories, current_category)
}

# Cap the diff shown per repo so a single large repo can't blow up the comment.
MAX_DIFF_LINES <- 300
OUT <- "ecosystem_comparison.md"

read_lines0 <- function(path) {
  if (!file.exists(path)) {
    return(character(0))
  }
  readLines(path, warn = FALSE)
}

# Number of files touched by a `git diff` (each starts with a "diff --git"
# header).
count_diff_files <- function(diff_lines) {
  sum(grepl("^diff --git ", diff_lines))
}

# Pull the file paths out of "Failed to format/read <path>: <err>" log lines.
extract_failures <- function(path) {
  lines <- read_lines0(path)
  hit <- grepl("^Failed to (format|read) ", lines)
  if (!any(hit)) {
    return(character(0))
  }
  unique(sub("^Failed to (format|read) ([^:]+): .*$", "\\2", lines[hit]))
}

# Renders one direction's diff into a collapsible block, capped so a single
# large repo can't blow up the comment. Returns "" when the diff is empty.
render_diff <- function(diff_lines, heading) {
  if (length(diff_lines) == 0) {
    return("")
  }
  shown <- head(diff_lines, MAX_DIFF_LINES)
  truncated <- length(diff_lines) > MAX_DIFF_LINES
  paste0(
    heading,
    if (truncated) paste0(" (first ", MAX_DIFF_LINES, " lines)") else "",
    ":\n\n",
    # A 4-backtick fence so the (rare) backtick in an R diff line can't close
    # the block early.
    "````diff\n",
    paste(shown, collapse = "\n"),
    "\n````\n\n"
  )
}

total_repos_diff <- 0
body <- character(0)
last_printed_category <- NULL

for (i in seq_along(repo_names)) {
  repo <- repo_names[i]
  sha <- repo_shas[i]
  category <- repo_categories[i]
  repo_dir <- gsub("/", "_", repo)

  message("Processing results of ", repo)

  cur_vs_air <- read_lines0(paste0("results/", repo_dir, "_current_vs_air.diff"))
  air_vs_cur <- read_lines0(paste0("results/", repo_dir, "_air_vs_current.diff"))
  n_cur_vs_air <- count_diff_files(cur_vs_air)
  n_air_vs_cur <- count_diff_files(air_vs_cur)

  # Failures each tool hits on the *original* source. A file the current project
  # fails on but Air handled is a regression worth flagging.
  air_fail <- extract_failures(paste0("results/", repo_dir, "_air_stderr.txt"))
  cur_fail <- extract_failures(paste0("results/", repo_dir, "_current_stderr.txt"))
  new_fail <- setdiff(cur_fail, air_fail)

  has_diff <- length(cur_vs_air) > 0 || length(air_vs_cur) > 0
  if (!has_diff && length(new_fail) == 0) {
    next
  }

  if (has_diff) {
    total_repos_diff <- total_repos_diff + 1
  }

  # Add a category subheader the first time a repo with changes shows up in
  # that category.
  if (!is.na(category) && !identical(last_printed_category, category)) {
    body <- c(body, paste0("## ", category, "\n\n"))
    last_printed_category <- category
  }

  summary <- paste0(
    "current→Air ",
    n_air_vs_cur,
    " file(s), Air→current ",
    n_cur_vs_air,
    " file(s) differ"
  )
  if (length(new_fail) > 0) {
    summary <- paste0(
      summary,
      ", ",
      length(new_fail),
      " new parse/format failure(s)"
    )
  }

  section <- paste0(
    "<details><summary><a href=\"https://github.com/",
    repo,
    "/tree/",
    sha,
    "\">",
    repo,
    "</a>: ",
    summary,
    "</summary>\n\n"
  )

  if (length(new_fail) > 0) {
    section <- paste0(
      section,
      "The current project fails to parse/format these files that Air handled ",
      "(first 50):\n\n",
      paste0("- `", head(new_fail, 50), "`", collapse = "\n"),
      "\n\n"
    )
  }

  section <- paste0(
    section,
    render_diff(cur_vs_air, "Current project applied on top of Air's output"),
    render_diff(air_vs_cur, "Air applied on top of the current project's output"),
    "</details>\n\n"
  )
  body <- c(body, section)
}

cat("# Ecosystem results\n\n", file = OUT)
cat(
  "Each repository is formatted by reference Air (latest release) and the ",
  "current project in both orders; a diff below is where one tool changes the ",
  "other's output.\n\n",
  sep = "",
  file = OUT,
  append = TRUE
)

if (length(body) == 0) {
  cat(
    "✅ The current project produced byte-for-byte identical output to Air ",
    "on every repository, and introduced no new parse/format failures.\n",
    sep = "",
    file = OUT,
    append = TRUE
  )
} else {
  cat(
    total_repos_diff,
    " repositories formatted differently\n\n",
    sep = "",
    file = OUT,
    append = TRUE
  )
  cat(paste(body, collapse = ""), file = OUT, append = TRUE)
}
