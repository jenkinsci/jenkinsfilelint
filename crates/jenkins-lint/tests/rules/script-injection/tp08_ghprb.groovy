node {
    stage('Report') {
        sh "echo Testing ${ghprbPullTitle}"
        sh 'sed -i -e "s#TEMPLATE#' + env.ghprbPullTitle + '#g" report.html'
        sh "git checkout ${params.ghprbSourceBranch}"
        def flags = env.ghprbCommentBody?.replace("\\", "")
        sh "python3 parse.py ${flags}"
        sh "git log --author='${ghprbPullAuthorLogin}'"
    }
}
