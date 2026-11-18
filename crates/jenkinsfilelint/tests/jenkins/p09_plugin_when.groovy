pipeline {
    agent any
    stages {
        stage('S') {
            when { gitlabMergeRequest() }
            steps { echo 'x' }
        }
    }
}
