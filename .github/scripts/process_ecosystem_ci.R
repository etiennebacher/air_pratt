# Summarizes the difference between the reference Air binary's formatted output
# and the current project's, across a set of repositories.
#
# The workflow formats the original source of each repo with each tool
# independently and diffs the two results (see the workflow comment for why we
# never run one tool on the other's output). For each repo it produces:
#   <repo>.diff               -> `git diff` of Air's output vs the current
#                                project's (empty == identical output)
#   <repo>_air_stderr.txt     -> Air's parse/format failures
#   <repo>_current_stderr.txt -> the current project's parse/format failures
#
# A non-empty diff is a place the two formatters produce different output for
# the same code -- something `--check` cannot reveal.

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

# Pull the file paths out of "Failed to format/read <path>: <err>" log lines.
extract_failures <- function(path) {
  lines <- read_lines0(path)
  hit <- grepl("^Failed to (format|read) ", lines)
  if (!any(hit)) {
    return(character(0))
  }
  unique(sub("^Failed to (format|read) ([^:]+): .*$", "\\2", lines[hit]))
}

total_repos_diff <- 0
total_files_diff <- 0
body <- character(0)
last_printed_category <- NULL

for (i in seq_along(repo_names)) {
  repo <- repo_names[i]
  sha <- repo_shas[i]
  category <- repo_categories[i]
  repo_dir <- gsub("/", "_", repo)

  message("Processing results of ", repo)

  diff_lines <- read_lines0(paste0("results/", repo_dir, ".diff"))
  # Each changed file starts with a "diff --git a/... b/..." header.
  changed_files <- sum(grepl("^diff --git ", diff_lines))

  air_fail <- extract_failures(paste0("results/", repo_dir, "_air_stderr.txt"))
  cur_fail <- extract_failures(paste0("results/", repo_dir, "_current_stderr.txt"))
  # Files the current project fails on but Air handled -> regressions worth
  # flagging (a failed file leaves no diff, so it wouldn't show up otherwise).
  new_fail <- setdiff(cur_fail, air_fail)

  if (length(diff_lines) == 0 && length(new_fail) == 0) {
    next
  }

  if (length(diff_lines) > 0) {
    total_repos_diff <- total_repos_diff + 1
    total_files_diff <- total_files_diff + changed_files
  }

  # Add a category subheader the first time a repo with changes shows up in
  # that category.
  if (!is.na(category) && !identical(last_printed_category, category)) {
    body <- c(body, paste0("## ", category, "\n\n"))
    last_printed_category <- category
  }

  summary <- paste0(changed_files, " file(s) formatted differently")
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

  if (length(diff_lines) > 0) {
    shown <- head(diff_lines, MAX_DIFF_LINES)
    truncated <- length(diff_lines) > MAX_DIFF_LINES
    # A 4-backtick fence so the (rare) backtick in an R diff line can't close
    # the block early. The diff is Air's output (a/) vs the current project's
    # (b/), so `-` lines are Air and `+` lines are the current project.
    section <- paste0(
      section,
      "Diff of Air's output (`-`) vs the current project's (`+`)",
      if (truncated) paste0(" (first ", MAX_DIFF_LINES, " lines)") else "",
      ":\n\n````diff\n",
      paste(shown, collapse = "\n"),
      "\n````\n\n"
    )
  }

  section <- paste0(section, "</details>\n\n")
  body <- c(body, section)
}

cat("# Ecosystem results\n\n", file = OUT)
cat(
  "The original source of each repository is formatted by reference Air (latest ",
  "release) and by the current project independently; a diff below is where ",
  "their output differs.\n\n",
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
    total_files_diff,
    " file(s) across ",
    total_repos_diff,
    " repositories formatted differently\n\n",
    sep = "",
    file = OUT,
    append = TRUE
  )
  cat(paste(body, collapse = ""), file = OUT, append = TRUE)
}
