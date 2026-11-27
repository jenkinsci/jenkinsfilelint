pipeline {
    agent any
    stages {
        stage('Notify') {
            steps {
                echo "Building ${env.CHANGE_TITLE}"
                writeFile file: 'title.txt', text: "${env.CHANGE_TITLE}"
                withEnv(["PR_TITLE=${env.CHANGE_TITLE}", "VERSION=${params.VERSION}"]) {
                    sh 'echo "$PR_TITLE" "$VERSION"'
                }
                script {
                    currentBuild.description = "${env.CHANGE_TITLE}"
                }
            }
        }
    }
}
