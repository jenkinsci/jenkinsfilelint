pipeline {
    agent any
    stages {
        stage('One') { steps { echo 'a'; echo 'b' } }
        stage("Two ${'x'}") { steps { sh "echo ${env.BUILD_ID}" } }
    }
}
