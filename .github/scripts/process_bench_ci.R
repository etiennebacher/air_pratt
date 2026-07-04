suppressPackageStartupMessages({
  library(data.table)
  library(jsonlite)
  library(tinytable)
})

repos_raw <- Sys.getenv("TEST_REPOS")
repo_lines <- strsplit(repos_raw, "\n")[[1]]
repo_lines <- repo_lines[repo_lines != ""]
repo_parts <- strsplit(repo_lines, "@")
all_repos <- setNames(
  lapply(repo_parts, function(x) trimws(x[2])), # the commit SHAs
  sapply(repo_parts, function(x) trimws(x[1])) # the repo names
)

cat("### Benchmark on real projects\n\n", file = "benchmark.md")
cat(
  "Run time of `air format --check .` on each repository: reference Air ",
  "(latest release) vs. the current project.\n\n",
  sep = "",
  file = "benchmark.md",
  append = TRUE
)

list_results <- list()

for (i in seq_along(all_repos)) {
  repos <- names(all_repos)[i]

  message("Processing results of ", repos)
  air_results_json <- jsonlite::read_json(paste0(
    "results_bench/",
    gsub("/", "_", repos),
    "_air.json"
  ))[["results"]][[1]][["times"]]
  current_results_json <- jsonlite::read_json(paste0(
    "results_bench/",
    gsub("/", "_", repos),
    "_current.json"
  ))[["results"]][[1]][["times"]]

  air_mean <- mean(unlist(air_results_json))
  current_mean <- mean(unlist(current_results_json))

  list_results[[i]] <- data.frame(
    Repository = repos,
    "Avg. duration (Air, seconds)" = air_mean,
    "Avg. duration (current, seconds)" = current_mean,
    "current - Air" = current_mean - air_mean,
    "current - Air (%)" = (current_mean - air_mean) / air_mean * 100,
    "Number of iterations" = length(air_results_json),
    check.names = FALSE
  )
}

all_results <- rbindlist(list_results)

tt(all_results) |>
  theme_markdown(style = "gfm") |>
  save_tt(output = "markdown") |>
  cat(file = "benchmark.md", append = TRUE)
