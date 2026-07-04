# Compares the formatting decisions of the reference Air binary and the current
# project's binary across a set of repositories.
#
# For each repo the workflow produces, for each tool ("air" = reference,
# "current" = this project), two files:
#   <repo>_<tool>_reformat.txt -> paths the tool would reformat
#   <repo>_<tool>_failed.txt   -> paths the tool failed to parse/format
#
# We report, per repo, the files where the two tools disagree:
#   - reformat divergences: one tool would reformat a file, the other wouldn't
#   - failure divergences:  one tool fails to process a file, the other doesn't
#
# `--check` only tells us *whether* a file would change, not *how*, so files
# that both tools would reformat are not compared here.

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

read_set <- function(path) {
  if (!file.exists(path)) {
    return(character(0))
  }
  x <- trimws(readLines(path, warn = FALSE))
  x[x != ""]
}

# Builds a <pre> block of links to the given files under the repo/sha.
file_links <- function(files, repo, sha) {
  files <- head(files, 50)
  paste0(
    "<a href=\"https://github.com/",
    repo,
    "/tree/",
    sha,
    "/",
    files,
    "\">",
    files,
    "</a>",
    collapse = "\n"
  )
}

total_reformat_diffs <- 0
total_failure_diffs <- 0
body <- character(0)
last_printed_category <- NULL

for (i in seq_along(repo_names)) {
  repo <- repo_names[i]
  sha <- repo_shas[i]
  category <- repo_categories[i]
  repo_dir <- gsub("/", "_", repo)

  message("Processing results of ", repo)

  air_reformat <- read_set(paste0("results/", repo_dir, "_air_reformat.txt"))
  cur_reformat <- read_set(paste0("results/", repo_dir, "_current_reformat.txt"))
  air_failed <- read_set(paste0("results/", repo_dir, "_air_failed.txt"))
  cur_failed <- read_set(paste0("results/", repo_dir, "_current_failed.txt"))

  # Reformat decisions where the two tools disagree.
  only_air_reformat <- setdiff(air_reformat, cur_reformat)
  only_cur_reformat <- setdiff(cur_reformat, air_reformat)

  # Files the current project fails on but Air handles are regressions; the
  # reverse means the current project now handles a file Air can't.
  new_failures <- setdiff(cur_failed, air_failed)
  fixed_failures <- setdiff(air_failed, cur_failed)

  n_diffs <- length(only_air_reformat) +
    length(only_cur_reformat) +
    length(new_failures) +
    length(fixed_failures)

  if (n_diffs == 0) {
    next
  }

  total_reformat_diffs <- total_reformat_diffs +
    length(only_air_reformat) +
    length(only_cur_reformat)
  total_failure_diffs <- total_failure_diffs +
    length(new_failures) +
    length(fixed_failures)

  # Add a category subheader the first time a repo with changes shows up in
  # that category.
  if (!is.na(category) && !identical(last_printed_category, category)) {
    body <- c(body, paste0("## ", category, "\n\n"))
    last_printed_category <- category
  }

  summary_line <- paste0(
    "reformat Δ ",
    length(only_air_reformat) + length(only_cur_reformat),
    ", failure Δ ",
    length(new_failures) + length(fixed_failures)
  )

  section <- paste0(
    "<details><summary><a href=\"https://github.com/",
    repo,
    "/tree/",
    sha,
    "\">",
    repo,
    "</a>: ",
    summary_line,
    "</summary>\n\n"
  )

  if (length(only_cur_reformat) > 0) {
    section <- paste0(
      section,
      "Current project would reformat, Air would not (first 50):<pre>",
      file_links(only_cur_reformat, repo, sha),
      "</pre>\n\n"
    )
  }
  if (length(only_air_reformat) > 0) {
    section <- paste0(
      section,
      "Air would reformat, current project would not (first 50):<pre>",
      file_links(only_air_reformat, repo, sha),
      "</pre>\n\n"
    )
  }
  if (length(new_failures) > 0) {
    section <- paste0(
      section,
      "Current project fails to parse/format, Air succeeds (first 50):<pre>",
      file_links(new_failures, repo, sha),
      "</pre>\n\n"
    )
  }
  if (length(fixed_failures) > 0) {
    section <- paste0(
      section,
      "Air fails to parse/format, current project succeeds (first 50):<pre>",
      file_links(fixed_failures, repo, sha),
      "</pre>\n\n"
    )
  }

  section <- paste0(section, "</details>\n\n")
  body <- c(body, section)
}

cat("# Ecosystem results\n\n", file = "ecosystem_comparison.md")
cat(
  "Comparison of `air format --check .` between reference Air (",
  Sys.getenv("AIR_VERSION"),
  ") and the current project. `--check` only reports whether a file would be\n",
  "reformatted, so files that *both* tools would reformat are not compared at\n",
  "the output level.\n\n",
  sep = "",
  file = "ecosystem_comparison.md",
  append = TRUE
)

if (length(body) == 0) {
  cat(
    "✅ No divergences: both tools would reformat the same files and neither ",
    "introduces new parse/format failures.\n",
    sep = "",
    file = "ecosystem_comparison.md",
    append = TRUE
  )
} else {
  cat(
    total_reformat_diffs,
    " divergent reformat decisions, ",
    total_failure_diffs,
    " divergent parse/format failures\n\n",
    sep = "",
    file = "ecosystem_comparison.md",
    append = TRUE
  )
  cat(
    paste(body, collapse = ""),
    file = "ecosystem_comparison.md",
    append = TRUE
  )
}
