pipeline {
    agent any
    triggers {
        githubPush()
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
