// vars/buildOnController.groovy
def call(Map config = [:]) {
    node('master') {
        stage('Seed') {
            sh "ls ${env.JENKINS_HOME}/jobs"
        }
    }
}
