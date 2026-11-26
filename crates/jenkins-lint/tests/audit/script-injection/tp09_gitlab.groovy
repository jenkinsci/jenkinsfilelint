pipeline {
    agent any
    stages {
        stage('Merge') {
            steps {
                sh "./merge.sh --src-branch ${env.gitlabSourceBranch} --target-branch ${env.gitlabTargetBranch}"
                sh "sed -i -e 's#TITLE#${env.gitlabMergeRequestTitle}#' report.html"
                sh "git config user.email ${env.gitlabUserEmail}"
                sh "git config user.name ${env.gitlabUserUsername}"
            }
        }
    }
}
